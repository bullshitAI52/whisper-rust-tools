use anyhow::Result;
use serde::{Deserialize, Serialize};
use reqwest::Client;
// use log::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: String,
    pub end: String,
    pub text: String,
}

pub struct DeepSeekClient {
    client: Client,
    api_key: String,
}

impl DeepSeekClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    /// Analyze content to extract segments (for Media Cutter)
    pub async fn analyze_segments(&self, prompt: &str, content: &str) -> Result<Vec<Segment>> {
        if self.api_key.is_empty() {
             return Err(anyhow::anyhow!("DeepSeek API Key is empty"));
        }
        
        // Construct the system/user prompt
        // Similar to Python script logic
        let system_prompt = "You are a professional video editor assistant. Extract interesting segments.";
        let full_prompt = format!("{}\n\nVideo Content/Context: {}\n\nReturn JSON array: [{{ \"start\": \"HH:MM:SS,mmm\", \"end\": \"HH:MM:SS,mmm\", \"text\": \"description\" }}]", prompt, content);

        let req_body = serde_json::json!({
            "model": "deepseek-chat",
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": full_prompt}
            ],
            "temperature": 0.3
        });

        let res = self.client.post("https://api.deepseek.com/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req_body)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await?;
            return Err(anyhow::anyhow!("API Error {}: {}", status, text));
        }

        let body: serde_json::Value = res.json().await?;
        let content = body["choices"][0]["message"]["content"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No content in response"))?;

        // Extract JSON from markdown code blocks if present
        let json_str = if let Some(start) = content.find('[') {
            if let Some(end) = content.rfind(']') {
                &content[start..=end]
            } else {
                content
            }
        } else {
            content
        };

        let segments: Vec<Segment> = serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse segments JSON: {}\nRaw: {}", e, content))?;

        Ok(segments)
    }

    /// Translate text (for Whisper App)
    pub async fn translate(&self, text: &str, target_lang: &str) -> Result<String> {
        if self.api_key.is_empty() {
            return Err(anyhow::anyhow!("DeepSeek API Key is empty"));
        }

        let full_prompt = format!("Translate the following subtitle text to {}. Maintain the original tone and SRT formatting style if possible (but just return text).\n\nText:\n{}", target_lang, text);

        let req_body = serde_json::json!({
            "model": "deepseek-chat",
            "messages": [
                {"role": "user", "content": full_prompt}
            ]
        });

        let res = self.client.post("https://api.deepseek.com/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req_body)
            .send()
            .await?;

        if !res.status().is_success() {
             return Err(anyhow::anyhow!("API Error: {}", res.status()));
        }

        let body: serde_json::Value = res.json().await?;
        let content = body["choices"][0]["message"]["content"].as_str()
            .unwrap_or("Thinking...")
            .trim()
            .to_string();

        Ok(content)
    }
    
    /// Generate storyboard prompts (for Whisper App)
    pub async fn generate_storyboard(&self, content: &str) -> Result<String> {
        if self.api_key.is_empty() {
             return Err(anyhow::anyhow!("DeepSeek API Key is empty"));
        }
        
        let full_prompt = format!("Generate a detailed Midjourney AI drawing prompt based on this text. Describe the scene, lighting, style (Cinematic, 8k). Return ONLY the prompt.\n\nContext:\n{}", content);

        let req_body = serde_json::json!({
            "model": "deepseek-chat",
            "messages": [
                {"role": "user", "content": full_prompt}
            ]
        });

        let res = self.client.post("https://api.deepseek.com/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req_body)
            .send()
            .await?;
            
        if !res.status().is_success() {
             return Err(anyhow::anyhow!("API Error: {}", res.status()));
        }

        let body: serde_json::Value = res.json().await?;
        let prompt_text = body["choices"][0]["message"]["content"].as_str()
            .unwrap_or("Failed to generate")
            .trim()
            .to_string();
            
        Ok(prompt_text)
    }
}
