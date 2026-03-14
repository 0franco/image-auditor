use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::config::{LlmConfig, LlmProvider};

#[derive(Debug, Clone)]
pub struct SuggestedPatch {
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone)]
pub struct LlmSuggestion {
    pub text: String,
    pub patch: Option<SuggestedPatch>,
}

pub trait LlmClient: Send + Sync {
    fn suggest_fix(&self, prompt: &str) -> Result<LlmSuggestion>;
}

pub fn create_llm_client(cfg: &LlmConfig) -> Result<Arc<dyn LlmClient>> {
    match cfg.provider {
        LlmProvider::OpenAi => Ok(Arc::new(OpenAiClient::new(cfg.clone())?)),
        LlmProvider::Anthropic => Ok(Arc::new(AnthropicClient::new(cfg.clone())?)),
        LlmProvider::Ollama => Ok(Arc::new(OllamaClient::new(cfg.clone())?)),
    }
}

fn parse_suggestion_with_patch(raw: &str) -> LlmSuggestion {
    // Very lightweight parser for the ---PATCH--- block described in the plan.
    if let Some(start) = raw.find("---PATCH---") {
        if let Some(end) = raw[start..].find("---END_PATCH---") {
            let block = &raw[start..start + end];
            let mut before = String::new();
            let mut after = String::new();

            let before_marker = "BEFORE:";
            let end_before = "---END_BEFORE---";
            let after_marker = "AFTER:";
            let end_after = "---END_AFTER---";

            if let Some(bm) = block.find(before_marker) {
                if let Some(eb) = block[bm..].find(end_before) {
                    let body = &block[bm + before_marker.len()..bm + eb];
                    before = body.trim_matches('\n').to_string();
                }
            }

            if let Some(am) = block.find(after_marker) {
                if let Some(ea) = block[am..].find(end_after) {
                    let body = &block[am + after_marker.len()..am + ea];
                    after = body.trim_matches('\n').to_string();
                }
            }

            let patch = if !before.is_empty() && !after.is_empty() {
                Some(SuggestedPatch {
                    before,
                    after,
                })
            } else {
                None
            };

            return LlmSuggestion {
                text: raw.to_string(),
                patch,
            };
        }
    }

    LlmSuggestion {
        text: raw.to_string(),
        patch: None,
    }
}

// ─── OpenAI ───────────────────────────────────────────────────────────────────

pub struct OpenAiClient {
    cfg: LlmConfig,
    http: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

impl OpenAiClient {
    pub fn new(cfg: LlmConfig) -> Result<Self> {
        let http = reqwest::blocking::Client::new();
        Ok(Self { cfg, http })
    }
}

impl LlmClient for OpenAiClient {
    fn suggest_fix(&self, prompt: &str) -> Result<LlmSuggestion> {
        let api_key = self
            .cfg
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow!("OPENAI_API_KEY is required for OpenAI provider"))?;

        let base = self
            .cfg
            .endpoint
            .clone()
            .unwrap_or_else(|| "https://api.openai.com".to_string());

        let url = format!("{}/v1/chat/completions", base);

        let body = serde_json::json!({
            "model": self.cfg.model,
            "messages": [
                { "role": "system", "content": "You are an assistant that suggests safe, concise image-related HTML/JS code fixes and emits an optional structured patch block as described." },
                { "role": "user", "content": prompt },
            ],
        });

        let resp = self
            .http
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()?
            .error_for_status()?;

        let parsed: OpenAiResponse = resp.json()?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("OpenAI returned no choices"))?
            .message
            .content;

        Ok(parse_suggestion_with_patch(&content))
    }
}

// ─── Anthropic ────────────────────────────────────────────────────────────────

pub struct AnthropicClient {
    cfg: LlmConfig,
    http: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

impl AnthropicClient {
    pub fn new(cfg: LlmConfig) -> Result<Self> {
        let http = reqwest::blocking::Client::new();
        Ok(Self { cfg, http })
    }
}

impl LlmClient for AnthropicClient {
    fn suggest_fix(&self, prompt: &str) -> Result<LlmSuggestion> {
        let api_key = self
            .cfg
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow!("ANTHROPIC_API_KEY is required for Anthropic provider"))?;

        let base = self
            .cfg
            .endpoint
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());

        let url = format!("{}/v1/messages", base);

        let body = serde_json::json!({
            "model": self.cfg.model,
            "max_tokens": 512u32,
            "messages": [
                { "role": "user", "content": prompt }
            ]
        });

        let resp = self
            .http
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()?
            .error_for_status()?;

        let parsed: AnthropicResponse = resp.json()?;
        let combined = parsed
            .content
            .into_iter()
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(parse_suggestion_with_patch(&combined))
    }
}

// ─── Ollama ───────────────────────────────────────────────────────────────────

pub struct OllamaClient {
    cfg: LlmConfig,
    http: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: String,
}

impl OllamaClient {
    pub fn new(cfg: LlmConfig) -> Result<Self> {
        let http = reqwest::blocking::Client::new();
        Ok(Self { cfg, http })
    }
}

impl LlmClient for OllamaClient {
    fn suggest_fix(&self, prompt: &str) -> Result<LlmSuggestion> {
        let base = self
            .cfg
            .endpoint
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        let url = format!("{}/api/chat", base);

        let body = serde_json::json!({
            "model": self.cfg.model,
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "stream": false,
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()?
            .error_for_status()?;

        let parsed: OllamaResponse = resp.json()?;
        Ok(parse_suggestion_with_patch(&parsed.message.content))
    }
}

