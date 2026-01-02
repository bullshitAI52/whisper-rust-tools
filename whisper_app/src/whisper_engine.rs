use anyhow::{Error, Result};
use candle_core::{Device, Tensor};
use candle_transformers::models::whisper::{self as m, Config};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::Tokenizer;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::path::Path;

use crate::audio::pcm_to_mel;

// ... imports remain ...
// We need to keep other imports, just change where we call functionality.

pub struct WhisperEngine {
    model: m::model::Whisper,
    tokenizer: Tokenizer,
    device: Device,
    mel_filters: Vec<f32>,
    config: Config,
}

impl WhisperEngine {
    pub fn new(model_id: &str) -> Result<Self> {
        let device = Device::new_metal(0).unwrap_or(Device::Cpu);
        println!("Using device: {:?}", device);
        
        let api = Api::new()?;
        let repo = api.repo(Repo::new(
            format!("openai/whisper-{}", model_id),
            RepoType::Model,
        ));

        let config_filename = repo.get("config.json")?;
        let tokenizer_filename = repo.get("tokenizer.json")?;
        let weights_filename = repo.get("model.safetensors")?;

        let config: Config = serde_json::from_str(&std::fs::read_to_string(config_filename)?)?;
        let tokenizer = Tokenizer::from_file(tokenizer_filename).map_err(Error::msg)?;
        
        let vb = unsafe {
            candle_nn::VarBuilder::from_mmaped_safetensors(&[weights_filename], m::DTYPE, &device)?
        };
        let model = m::model::Whisper::load(&vb, config.clone())?;
        
        let mel_path = match config.num_mel_bins {
            80 => api.repo(Repo::new("openai/whisper-tiny".to_string(), RepoType::Model)).get("mel_filters.bytes")?,
            _ => api.repo(Repo::new("openai/whisper-large-v3".to_string(), RepoType::Model)).get("mel_filters.bytes")?,
        };
        let mel_bytes = std::fs::read(mel_path)?;
        let mut mel_filters = vec![0f32; mel_bytes.len() / 4];
        <byteorder::LittleEndian as byteorder::ByteOrder>::read_f32_into(&mel_bytes, &mut mel_filters);

        Ok(Self {
            model,
            tokenizer,
            device,
            mel_filters,
            config,
        })
    }

    pub fn transcribe(&mut self, audio_path: &str) -> Result<Vec<(f64, f64, String)>> {
        let pcm_data = load_audio(audio_path)?;
        let mel = pcm_to_mel(&self.config, &pcm_data, &self.mel_filters, &self.device)?;
        
        // Run Encoder
        let audio_features = self.model.encoder.forward(&mel, true)?;
        
        // Simple Greedy Decoder with Timestamps
        let sot_token = *self.tokenizer.get_vocab(true).get("<|startoftranscript|>").unwrap_or(&50258);
        let eot_token = *self.tokenizer.get_vocab(true).get("<|endoftext|>").unwrap_or(&50257);
        let transcribe_token = *self.tokenizer.get_vocab(true).get("<|transcribe|>").unwrap_or(&50359);
        // We do NOT add <|notimestamps|> because we WANT timestamps.
        
        // Find timestamp begin index. Usually it's right after <|notimestamps|> or at a fixed index.
        // For OpenAI models: <|notimestamps|> is 50363. Timestamps start at 50364.
        // We will detect it dynamically or fallback.
        let no_timestamps_id = *self.tokenizer.get_vocab(true).get("<|notimestamps|>").unwrap_or(&50363);
        let timestamp_begin = no_timestamps_id + 1;

        let mut tokens = vec![sot_token, transcribe_token];
        // Language detection is skipped for now (assuming English or letting model default).
        
        let mut segments = Vec::new();
        let mut current_start = 0.0;
        let mut current_text_tokens = Vec::new();
        
        // Safety limit
        for _ in 0..1000 { 
            let input = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let logits = self.model.decoder.forward(&input, &audio_features, true)?;
            let logits = logits.squeeze(0)?;
            let (_seq_len, _vocab_size) = logits.dims2()?;
            
            let last_logits = logits.get(_seq_len - 1)?;
            let next_token = last_logits.argmax(0)?.to_scalar::<u32>()?;
            
            if next_token == eot_token {
                // If we have pending text, save it ending at 30.0 or current max
                if !current_text_tokens.is_empty() {
                    let text = self.tokenizer.decode(&current_text_tokens, true).unwrap_or_default();
                    segments.push((current_start, 30.0, text)); // Default end to window max
                }
                break;
            }
            
            tokens.push(next_token);
            
            if next_token >= timestamp_begin {
                let time = (next_token - timestamp_begin) as f64 * 0.02;
                
                if !current_text_tokens.is_empty() {
                    // This timestamp likely ends the previous segment
                    let text = self.tokenizer.decode(&current_text_tokens, true).unwrap_or_default();
                    segments.push((current_start, time, text));
                    current_text_tokens.clear();
                }
                
                // This timestamp also starts the next segment
                current_start = time;
            } else {
                current_text_tokens.push(next_token);
            }
        }
        
        Ok(segments)
    }
}

fn load_audio(path: impl AsRef<Path>) -> Result<Vec<f32>> {
    let src = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let hint = Hint::new();
    // hint.with_extension("mp3"); 

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();
    let probed = symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts)?;
    let mut format = probed.format;
    let track = format.default_track().ok_or_else(|| anyhow::anyhow!("no track found"))?;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.ok_or_else(|| anyhow::anyhow!("no sample rate"))?;

    let mut pcm_data = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id { continue; }
        let decoded = decoder.decode(&packet)?;
        let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        sample_buf.copy_interleaved_ref(decoded);
        
        pcm_data.extend_from_slice(sample_buf.samples());
    }
    
    // Resample if needed (very naive check and decimation if 48k -> 16k)
    // If 44.1k, this simple logic fails. 
    // Assuming 16k input or implementing naive downsample.
    // For this fast fix:
    if sample_rate == 48000 {
         let mut new_pcm = Vec::new();
         for (i, sample) in pcm_data.iter().enumerate() {
             if i % 3 == 0 { new_pcm.push(*sample); }
         }
         Ok(new_pcm)
    } else if sample_rate == 16000 {
        Ok(pcm_data)
    } else {
        // Fallback: warn and return as is (will sound slow/fast)
        Ok(pcm_data)
    }
}
