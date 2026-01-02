use anyhow::Result;
use std::process::Command;
use std::path::Path;

pub struct VideoCutter;

impl VideoCutter {
    pub fn cut_segment(input: &str, start: &str, end: &str, output: &str, reencode: bool, crf: &str, preset: &str, mute: bool) -> Result<()> {
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y")
           .arg("-i").arg(input)
           .arg("-ss").arg(start)
           .arg("-to").arg(end);
           
        if mute {
            cmd.arg("-an");
        }

        if !reencode {
            cmd.arg("-c").arg("copy");
        } else {
            // Re-encode with user params
            cmd.arg("-c:v").arg("libx264")
               .arg("-crf").arg(crf)
               .arg("-preset").arg(preset);
               
            if !mute {
                cmd.arg("-c:a").arg("aac");
            }
        }

        let status = cmd.arg(output).status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("FFmpeg failed"))
        }
    }
    pub fn get_duration(input: &str) -> Result<f64> {
        let output = Command::new("ffprobe")
            .arg("-v").arg("error")
            .arg("-show_entries").arg("format=duration")
            .arg("-of").arg("default=noprint_wrappers=1:nokey=1")
            .arg(input)
            .output()?;
            
        let duration_str = String::from_utf8(output.stdout)?;
        let duration = duration_str.trim().parse::<f64>()?;
        Ok(duration)
    }

    /// Burn subtitles into video (Hardcode)
    /// Strategies: 
    /// 1. Copy SRT to a temporary file in CWD to avoid complex path escaping in FFmpeg filter
    /// 2. Use `subtitles=filename.srt`
    pub fn burn_subtitles(input: &str, srt_path: &str, output: &str, crf: &str, preset: &str) -> Result<()> {
        // Create a unique temp name for the SRT in current dir
        let temp_srt_name = format!("temp_subs_{}.srt", uuid::Uuid::new_v4());
        std::fs::copy(srt_path, &temp_srt_name)?;
        
        let filter_arg = format!("subtitles={}", temp_srt_name);

        let status = Command::new("ffmpeg")
            .arg("-y")
            .arg("-i").arg(input)
            .arg("-vf").arg(&filter_arg)
            .arg("-c:v").arg("libx264")
            .arg("-crf").arg(crf)
            .arg("-preset").arg(preset)
            .arg("-c:a").arg("copy") // Copy audio, unless we want to process it too, but copy is safer/faster
            .arg(output)
            .status();

        // Cleanup temp file regardless of success
        let _ = std::fs::remove_file(&temp_srt_name);

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(anyhow::anyhow!("FFmpeg exited with error: {}", s)),
            Err(e) => Err(anyhow::anyhow!("Failed to execute FFmpeg: {}", e)),
        }
    }

    /// Extract audio to MP3
    pub fn extract_audio(input: &str, output: &str) -> Result<()> {
        let status = Command::new("ffmpeg")
            .arg("-y")
            .arg("-i").arg(input)
            .arg("-vn") // No video
            .arg("-acodec").arg("libmp3lame")
            .arg("-q:a").arg("2") // High quality
            .arg(output)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("FFmpeg audio extraction failed"))
        }
    }

    /// Merge multiple videos using concat demuxer
    pub fn merge_videos(inputs: &[String], output: &str) -> Result<()> {
        if inputs.is_empty() {
             return Err(anyhow::anyhow!("No input files to merge"));
        }

        // Create a temporary file list for ffmpeg concat
        let list_path = format!("concat_list_{}.txt", uuid::Uuid::new_v4());
        let mut list_content = String::new();
        for path in inputs {
            // Escape single quotes for ffmpeg
            let escaped_path = path.replace("'", "'\\''");
            list_content.push_str(&format!("file '{}'\n", escaped_path));
        }
        std::fs::write(&list_path, list_content)?;

        let status = Command::new("ffmpeg")
            .arg("-y")
            .arg("-f").arg("concat")
            .arg("-safe").arg("0")
            .arg("-i").arg(&list_path)
            .arg("-c").arg("copy") // Stream copy for speed
            .arg(output)
            .status();

        // Cleanup
        let _ = std::fs::remove_file(&list_path);

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(anyhow::anyhow!("FFmpeg merge failed: {}", s)),
            Err(e) => Err(anyhow::anyhow!("FFmpeg execution failed: {}", e)),
        }
    }

    /// Compress video with simple CRF strategy
    pub fn compress_video(input: &str, output: &str, crf: &str) -> Result<()> {
        let status = Command::new("ffmpeg")
            .arg("-y")
            .arg("-i").arg(input)
            .arg("-c:v").arg("libx264")
            .arg("-crf").arg(crf)
            .arg("-preset").arg("medium")
            .arg("-c:a").arg("aac")
            .arg("-b:a").arg("128k") // Compress audio too
            .arg(output)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("FFmpeg compression failed"))
        }
    }

    /// Convert media format
    pub fn convert_format(input: &str, output: &str) -> Result<()> {
        let status = Command::new("ffmpeg")
            .arg("-y")
            .arg("-i").arg(input)
            .arg("-c").arg("copy") // Try stream copy first for speed
            .arg("-strict").arg("experimental") 
            .arg(output)
            .status();

        match status {
             Ok(s) if s.success() => Ok(()),
             _ => {
                 // If copy fails (incompatible codecs), try re-encoding
                 let status_re = Command::new("ffmpeg")
                    .arg("-y")
                    .arg("-i").arg(input)
                    .arg(output)
                    .status()?;
                    
                 if status_re.success() {
                     Ok(())
                 } else {
                     Err(anyhow::anyhow!("FFmpeg conversion failed"))
                 }
             }
        }
    }
}
