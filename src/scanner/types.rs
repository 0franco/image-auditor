use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}

impl std::fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueSeverity::Error => write!(f, "ERROR"),
            IssueSeverity::Warning => write!(f, "WARN"),
            IssueSeverity::Info => write!(f, "INFO"),
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum IssueKind {
    WrongFormat,
    MissingAlt,
    MissingWidthHeight,
    MissingLazyLoading,
    OversizedFile,
    MissingSrcset,
}

impl std::fmt::Display for IssueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueKind::WrongFormat       => write!(f, "Wrong Format (not WebP/AVIF)"),
            IssueKind::MissingAlt        => write!(f, "Missing alt attribute"),
            IssueKind::MissingWidthHeight => write!(f, "Missing width/height"),
            IssueKind::MissingLazyLoading => write!(f, "Missing lazy loading"),
            IssueKind::OversizedFile     => write!(f, "Oversized image file"),
            IssueKind::MissingSrcset     => write!(f, "Missing srcset"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub kind: IssueKind,
    pub severity: IssueSeverity,
    pub file: PathBuf,
    pub line: usize,
    pub snippet: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScanResult {
    pub issues: Vec<Issue>,
    pub files_scanned: usize,
    pub images_found: usize,
}
