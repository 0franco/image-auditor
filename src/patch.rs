use anyhow::{anyhow, Result};

use crate::llm::SuggestedPatch;
use crate::scanner::Issue;

/// Apply a suggested patch by doing a literal string replacement in the file.
pub fn apply_suggested_patch(issue: &Issue, patch: &SuggestedPatch) -> Result<()> {
    let contents = std::fs::read_to_string(&issue.file)?;

    let before = patch.before.trim_matches('\n');
    let after = patch.after.trim_matches('\n');

    let idx = contents
        .find(before)
        .ok_or_else(|| anyhow!("Original snippet not found in file — it may have changed."))?;

    let mut new_contents = String::with_capacity(contents.len() + after.len());
    new_contents.push_str(&contents[..idx]);
    new_contents.push_str(after);
    new_contents.push_str(&contents[idx + before.len()..]);

    std::fs::write(&issue.file, new_contents)?;
    Ok(())
}
