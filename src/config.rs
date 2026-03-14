use std::env;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub enum LlmProvider {
    OpenAi,
    Anthropic,
    Ollama,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
    pub model: String,
}

pub fn load_llm_config() -> Result<Option<LlmConfig>> {
    let provider_raw = match env::var("ACTIVE_LLM_PROVIDER") {
        Ok(v) if !v.trim().is_empty() => v.to_lowercase(),
        _ => return Ok(None),
    };

    let provider = match provider_raw.as_str() {
        "openai" => LlmProvider::OpenAi,
        "anthropic" => LlmProvider::Anthropic,
        "ollama" => LlmProvider::Ollama,
        other => return Err(anyhow!("Unsupported ACTIVE_LLM_PROVIDER: {}", other)),
    };

    let (api_key, endpoint, model) = match provider {
        LlmProvider::OpenAi => {
            let key = env::var("OPENAI_API_KEY").ok();
            let endpoint = env::var("OPENAI_BASE_URL").ok();
            let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-5.2".to_string());
            (key, endpoint, model)
        }
        LlmProvider::Anthropic => {
            let key = env::var("ANTHROPIC_API_KEY").ok();
            let endpoint = env::var("ANTHROPIC_BASE_URL").ok();
            let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-5-sonnet-latest".to_string());
            (key, endpoint, model)
        }
        LlmProvider::Ollama => {
            let endpoint = env::var("OLLAMA_BASE_URL").ok();
            let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string());
            (None, endpoint, model)
        }
    };

    Ok(Some(LlmConfig { provider, api_key, endpoint, model }))
}

