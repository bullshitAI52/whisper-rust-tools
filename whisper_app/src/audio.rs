use anyhow::{Error, Result};
use candle_core::{Device, Tensor};
use rustfft::{FftPlanner, num_complex::Complex};
use std::f32::consts::PI;

pub const SAMPLE_RATE: usize = 16000;
pub const N_FFT: usize = 400;
pub const HOP_LENGTH: usize = 160;
pub const CHUNK_LENGTH: usize = 30;
pub const N_SAMPLES: usize = CHUNK_LENGTH * SAMPLE_RATE; // 480000
pub const N_FRAMES: usize = N_SAMPLES / HOP_LENGTH; // 3000

pub fn pcm_to_mel(
    config: &candle_transformers::models::whisper::Config,
    pcm: &[f32],
    mel_filters: &[f32],
    device: &Device
) -> Result<Tensor> {
    let mut pcm = pcm.to_vec();
    // 1. Pad or trim to 30 seconds
    if pcm.len() < N_SAMPLES {
        pcm.resize(N_SAMPLES, 0.0);
    } else {
        pcm.truncate(N_SAMPLES);
    }

    // 2. Hann Window
    let window = (0..N_FFT)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / N_FFT as f32).cos()))
        .collect::<Vec<_>>();

    // 3. STFT (Short-Time Fourier Transform)
    // We computed magnitudes: |FFT(window * frame)|^2
    // We ignore the DC component and keep first (N_FFT / 2 + 1) bins.
    
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(N_FFT);
    
    let mut magnitudes = Vec::with_capacity(N_FRAMES * (N_FFT / 2 + 1));
    
    for i in 0..N_FRAMES {
        let start = i * HOP_LENGTH;
        let end = start + N_FFT;
        let mut frame = vec![Complex { re: 0.0, im: 0.0 }; N_FFT];
        
        for j in 0..N_FFT {
            if start + j < pcm.len() {
                frame[j].re = pcm[start + j] * window[j];
            }
        }
        
        fft.process(&mut frame);
        
        // Take squared magnitude of the first (N_FFT / 2 + 1) coefficients
        for j in 0..=(N_FFT / 2) {
            let s = frame[j].re * frame[j].re + frame[j].im * frame[j].im;
            magnitudes.push(s);
        }
    }
    
    // 4. Mel Filterbank Application
    // mel_filters size: (n_mels * (N_FFT / 2 + 1)) flattened row-major?
    // Actually Whisper mel_filters.bytes are likely stored as (n_mels, n_fft_half)
    let n_mels = config.num_mel_bins;
    let n_fft_half = N_FFT / 2 + 1;
    
    let mag_tensor = Tensor::from_vec(magnitudes, (N_FRAMES, n_fft_half), device)?;
    let mel_filters_tensor = Tensor::from_vec(mel_filters.to_vec(), (n_mels, n_fft_half), device)?;
    
    // Result = Mel (n_mels, n_fft_half) @ Mag.T (n_fft_half, N_FRAMES) -> (n_mels, N_FRAMES)
    // Candle matmul: (m, k) @ (k, n) -> (m, n)
    // We need Mag.t()
    let mel_spec = mel_filters_tensor.matmul(&mag_tensor.t()?)?;
    
    // 5. Log10(max(x, 1e-10))
    let mel_spec = mel_spec
        .clamp(1e-10f64, f64::MAX)?
        .log()?
        .div(&Tensor::new(&[10.0f64.ln()], device)?)?
        // 6. Scale: (x + 4.0) / 4.0 ? No, standard Log10 is usually (log10(x) + 8.0) / 4.0? 
        // Wait, regular Whisper prep is log10(x) then clamp max=8?
        // Reference Python: log_spec = torch.clamp(log_spec, min=max(log_spec.max() - 8.0, -4.0)) -> (log_spec + 4.0) / 4.0
        // Actually Candle examples simplified this often to just log10?
        // Let's check `candle-transformers` whisper source code logic if possible or follow standard:
        // Standard Whisper behavior: 
        // log_spec = torch.log10(mel_spec)
        // log_spec = torch.maximum(log_spec, log_spec.max() - 8.0)
        // log_spec = (log_spec + 4.0) / 4.0
        // But often just log10 is returned if the model handles normalization?
        // Let's apply valid normalization:
        ;

    let max_val = mel_spec.max_keepdim(0)?.max_keepdim(1)?;
    let min_val = max_val.sub(&Tensor::new(&[8.0f64], device)?)?;
    let mel_spec = mel_spec.broadcast_maximum(&min_val)?;
    let mel_spec = mel_spec.add(&Tensor::new(&[4.0f64], device)?)?.div(&Tensor::new(&[4.0f64], device)?)?;
    
    // Add batch dimension: (1, n_mels, N_FRAMES)
    let mel_spec = mel_spec.unsqueeze(0)?;
    
    Ok(mel_spec)
}
