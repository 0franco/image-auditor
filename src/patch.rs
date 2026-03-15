use anyhow::{anyhow, Result};

use crate::llm::SuggestedPatch;
use crate::scanner::Issue;

/// Apply a suggested patch by doing a string replacement in the file.
///
/// Matching strategy (tried in order):
///   1. Exact byte match — fastest and most precise.
///   2. Whitespace-normalized match — collapses runs of whitespace/newlines on
///      both sides before comparing, then maps the match back to the original
///      file bytes. Handles cases where the LLM slightly re-indents the BEFORE
///      block (e.g. PHP string-concatenation tags, JSX multi-line attributes).
pub fn apply_suggested_patch(issue: &Issue, patch: &SuggestedPatch) -> Result<()> {
    let contents = std::fs::read_to_string(&issue.file)?;

    let before = patch.before.trim_matches('\n');
    let after  = patch.after.trim_matches('\n');

    // ── Strategy 1: exact match ───────────────────────────────────────────
    if let Some(idx) = contents.find(before) {
        return write_patch(&issue.file, &contents, idx, before.len(), after);
    }

    // ── Strategy 2: whitespace-normalized match ───────────────────────────
    // Collapse every run of whitespace (spaces, tabs, newlines) to a single
    // space, find that in the normalised file, then locate the corresponding
    // span in the *original* file content.
    let norm_file   = normalize_ws(&contents);
    let norm_before = normalize_ws(before);

    if let Some(norm_idx) = norm_file.find(&norm_before) {
        // Map the normalised byte offset back to the original file.
        if let Some((orig_start, orig_end)) = denormalize_span(&contents, norm_idx, norm_before.len()) {
            return write_patch(&issue.file, &contents, orig_start, orig_end - orig_start, after);
        }
    }

    Err(anyhow!(
        "Could not locate the original snippet in '{}' (tried exact and whitespace-normalized match).\n\
         The file may have been modified, or the LLM patch does not match the source.",
        issue.file.display()
    ))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_patch(path: &std::path::Path, contents: &str, start: usize, len: usize, after: &str) -> Result<()> {
    let mut new_contents = String::with_capacity(contents.len() + after.len());
    new_contents.push_str(&contents[..start]);
    new_contents.push_str(after);
    new_contents.push_str(&contents[start + len..]);
    std::fs::write(path, new_contents)?;
    Ok(())
}

/// Collapse every whitespace run to a single ASCII space.
fn normalize_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out
}

/// Given a byte offset + length in the *normalised* string, find the
/// corresponding byte range in the *original* string.
///
/// We walk both strings simultaneously, mapping normalised positions back to
/// original positions by counting how many original bytes produced each
/// normalised byte.
fn denormalize_span(original: &str, norm_start: usize, norm_len: usize) -> Option<(usize, usize)> {
    let norm_end = norm_start + norm_len;

    let mut orig_pos  = 0usize; // byte offset in `original`
    let mut norm_pos  = 0usize; // byte offset in normalised string
    let mut in_ws     = false;

    let mut orig_start = None;
    let mut orig_end   = None;

    let chars: Vec<(usize, char)> = original.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let (byte_idx, ch) = chars[i];

        if norm_pos == norm_start {
            orig_start = Some(byte_idx);
        }
        if norm_pos == norm_end {
            orig_end = Some(byte_idx);
            break;
        }

        if ch.is_whitespace() {
            if !in_ws {
                // This whitespace run maps to exactly one space in the norm string.
                norm_pos += 1; // one ' '
                in_ws = true;
            }
            // skip remaining whitespace chars in original
            orig_pos = byte_idx + ch.len_utf8();
            i += 1;
            continue;
        }

        in_ws = false;
        norm_pos += ch.len_utf8(); // same char in both strings
        orig_pos = byte_idx + ch.len_utf8();
        i += 1;
    }

    // Handle match reaching end of string
    if orig_end.is_none() && norm_pos == norm_end {
        orig_end = Some(orig_pos);
    }

    match (orig_start, orig_end) {
        (Some(s), Some(e)) => Some((s, e)),
        _ => None,
    }
}
