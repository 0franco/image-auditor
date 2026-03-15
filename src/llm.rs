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

/// Strip artefacts the LLM sometimes adds to BEFORE/AFTER blocks:
///   - Leading/trailing newlines
///   - Markdown code fences (```html, ```jsx, ``` etc.)
///   - Line-number prefixes like "   5: " or ">>>  5: " produced by our
///     context format (defensive — shouldn't appear, but just in case)
fn sanitize_patch_block(raw: &str) -> String {
    use std::sync::OnceLock;
    static LINE_PREFIX_RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = LINE_PREFIX_RE
        .get_or_init(|| regex::Regex::new(r"(?m)^(?:>>>|   )\s*\d+: ?").unwrap());

    let trimmed = raw.trim_matches('\n');

    // Strip opening code fence + language tag if present (```html, ```jsx, etc.)
    let inner = if let Some(rest) = trimmed.strip_prefix("```") {
        let after_lang = rest.find('\n').map(|i| &rest[i + 1..]).unwrap_or(rest);
        after_lang
            .strip_suffix("```")
            .map(|s| s.trim_matches('\n'))
            .unwrap_or(after_lang)
    } else {
        trimmed
    };

    // Strip line-number prefixes the LLM might have copied from our context display
    re.replace_all(inner, "").into_owned()
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
                    before = sanitize_patch_block(body);
                }
            }

            if let Some(am) = block.find(after_marker) {
                if let Some(ea) = block[am..].find(end_after) {
                    let body = &block[am + after_marker.len()..am + ea];
                    after = sanitize_patch_block(body);
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

// ─── Prompt builder ───────────────────────────────────────────────────────────

use crate::scanner::Issue;

pub fn build_issue_prompt(issue: &Issue) -> String {
    let file_path = issue.file.to_string_lossy();
    let file_type = issue
        .file
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    let context = read_file_context(&issue.file, issue.line, 4);

    let verbose = std::env::var("AI_VERBOSE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    // Extra guidance injected only for the MissingWidthHeight issue so the LLM
    // never falls back to width="0" height="0".
    let dim_hint = if matches!(issue.kind, crate::scanner::IssueKind::MissingWidthHeight) {
        dimension_hint()
    } else {
        ""
    };

    let patch_instructions = format!(
        "---PATCH---\n\
file: {file_path}\n\
BEFORE:\n\
<exact lines from the file that need changing — copy verbatim>\n\
---END_BEFORE---\n\
AFTER:\n\
<those same lines with ONLY the minimum change to fix the issue>\n\
---END_AFTER---\n\
---END_PATCH---"
    );

    if verbose {
        return format!(
            "You are fixing a specific image-delivery issue in {file_type} source code.\n\
File: {file_path}  Line: {line}\n\
Issue type: {kind}\n\
Diagnosis: {msg}\n\
{dim_hint}\n\
File context (line {line} is the target):\n\
{context}\n\
Rules:\n\
- Add or change ONLY what the diagnosis says is missing or wrong.\n\
- Do NOT add attributes that are already present in the code above.\n\
- Preserve all existing attributes, whitespace style, and quote style.\n\
- If the tag spans multiple lines keep the same formatting.\n\n\
Briefly explain the fix, then emit:\n\
{patch_instructions}",
            line = issue.line,
            kind = issue.kind,
            msg = issue.message,
        );
    }

    format!(
        "You are fixing a specific image-delivery issue in {file_type} source code.\n\
File: {file_path}  Line: {line}\n\
Issue type: {kind}\n\
Diagnosis: {msg}\n\
{dim_hint}\n\
File context (line {line} is the target):\n\
{context}\n\
Rules:\n\
- Add or change ONLY what the diagnosis says is missing or wrong.\n\
- Do NOT add attributes that are already present in the code above.\n\
- Preserve all existing attributes, whitespace style, and quote style.\n\n\
Output ONLY the patch block below, no prose, no markdown fences:\n\
{patch_instructions}\n\
If no safe patch is possible output exactly: NO_PATCH",
        line = issue.line,
        kind = issue.kind,
        msg = issue.message,
    )
}

/// Build a dimension-specific hint for the MissingWidthHeight issue.
fn dimension_hint() -> &'static str {
    "Width/height guidance:\n\
- Infer likely dimensions from the src path, filename, and CSS classes (e.g. 'banner' → 1200x400, 'avatar'/'icon' → 48x48, 'logo' → 160x50, 'hero' → 1280x720).\n\
- NEVER use 0 or 1 — those values break layout and are worse than omitting the attributes.\n\
- If you truly cannot infer reasonable dimensions, output NO_PATCH instead.\n"
}

/// Read `radius` lines before and after `target_line` (1-based) from `path`.
/// The target line is annotated separately so the LLM never sees the marker
/// as part of the line content it should copy into a patch BEFORE block.
fn read_file_context(path: &std::path::Path, target_line: usize, radius: usize) -> String {
    let Ok(content) = std::fs::read_to_string(path) else {
        return "(file not readable)".to_string();
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = target_line.saturating_sub(radius + 1);
    let end = (target_line + radius).min(lines.len());

    let mut out = format!("(line {} is the issue — copy it verbatim into BEFORE)\n", target_line);
    for (i, l) in lines[start..end].iter().enumerate() {
        let lineno = start + i + 1;
        out.push_str(&format!("{:4}: {}\n", lineno, l));
    }
    out
}
