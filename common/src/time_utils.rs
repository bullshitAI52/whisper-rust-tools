use regex::Regex;
use std::fmt;
use anyhow::{Context, Result};

pub fn time_str_to_seconds(time_str: &str) -> Result<f64> {
    if time_str.trim().is_empty() {
        return Ok(0.0);
    }
    let s = time_str.trim().replace('.', ",");
    
    // Regex to match HH:MM:SS,mmm or H:MM:SS.mmm
    // Corresponds to python: r'(\d+):(\d{1,2}):(\d{1,2})([.,](\d{1,3}))?$'
    let re = Regex::new(r"^(\d+):(\d{1,2}):(\d{1,2})([.,](\d{1,3}))?$")
        .context("Failed to compile regex")?;

    if let Some(captures) = re.captures(&s) {
        let h: u32 = captures.get(1).map_or("0", |m| m.as_str()).parse()?;
        let m: u32 = captures.get(2).map_or("0", |m| m.as_str()).parse()?;
        let sec: u32 = captures.get(3).map_or("0", |m| m.as_str()).parse()?;
        let ms_str = captures.get(5).map_or("0", |m| m.as_str());
        // pad right with 0s to ensure it's ms? No, python `ljust(3, '0')` suggests it treats "5" as 500ms if followed by comma?
        // Let's check python logic: 
        // ms = int((m.group(5) or '0').ljust(3, '0'))
        // If capture is "5", ljust(3, '0') -> "500". So "00:00:01,5" -> 1.5s
        let ms_padded = format!("{:0<3}", ms_str); 
        let ms: u32 = ms_padded.parse()?;
        
        Ok((h * 3600 + m * 60 + sec) as f64 + (ms as f64 / 1000.0))
    } else {
        Err(anyhow::anyhow!("Invalid time format: {}", time_str))
    }
}

pub fn seconds_to_time_str(seconds: f64) -> String {
    if seconds <= 0.0 {
        return "00:00:00,000".to_string();
    }
    let total_ms = (seconds * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_seconds = total_ms / 1000;
    let h = total_seconds / 3600;
    let m = (total_seconds % 3600) / 60;
    let s = total_seconds % 60;

    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}


pub fn format_time_for_filename(t_str: &str) -> String {
    if t_str.is_empty() {
        return "00_00_00".to_string();
    }
    // Python: t = t_str.split(',')[0].split('.')[0]
    // return t.replace(':', '_')
    let t = t_str.split(',').next().unwrap_or("")
             .split('.').next().unwrap_or("");
    t.replace(':', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_parsing() {
        assert_eq!(time_str_to_seconds("00:00:10,500").unwrap(), 10.5);
        assert_eq!(time_str_to_seconds("01:01:01,100").unwrap(), 3661.1);
        assert_eq!(time_str_to_seconds("0:00:10").unwrap(), 10.0);
    }
    
    #[test]
    fn test_time_formatting() {
        assert_eq!(seconds_to_time_str(10.5), "00:00:10,500");
    }
}
