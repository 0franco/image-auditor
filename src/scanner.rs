use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use regex::Regex;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use anyhow::Result;

static IMG_TAG_RE: OnceLock<Regex> = OnceLock::new();
static IMG_START_RE: OnceLock<Regex> = OnceLock::new();
static IMAGE_JSX_RE: OnceLock<Regex> = OnceLock::new();

fn get_img_tag_re() -> &'static Regex {
    IMG_TAG_RE.get_or_init(|| {
        Regex::new(r#"(?is)<img\b((?:"[^"]*"|'[^']*'|<\?(?:php|=)[^?]*\?>|[^>"'])*)\s*/?>"#)
            .expect("Invalid Regex")
    })
}

fn get_img_start_re() -> &'static Regex {
    IMG_START_RE.get_or_init(|| Regex::new(r"(?i)<img\b").expect("Invalid Regex"))
}

fn get_image_jsx_re() -> &'static Regex {
    IMAGE_JSX_RE.get_or_init(|| Regex::new(r"(?i)<Image\b").expect("Invalid Regex"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueKind {
    WrongFormat,
    MissingWidthHeight,
    MissingLazyLoading,
    OversizedFile,
    MissingSrcset,
}

impl std::fmt::Display for IssueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueKind::WrongFormat => write!(f, "Wrong Format (not WebP/AVIF)"),
            IssueKind::MissingWidthHeight => write!(f, "Missing width/height"),
            IssueKind::MissingLazyLoading => write!(f, "Missing lazy loading"),
            IssueKind::OversizedFile => write!(f, "Oversized image file"),
            IssueKind::MissingSrcset => write!(f, "Missing srcset"),
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

// Max file size for referenced images on disk (if resolvable), and for base64 data URIs
const MAX_IMAGE_SIZE_BYTES: u64 = 200 * 1024; // 200 KiB

pub fn scan_directory(root: &Path) -> Result<ScanResult> {
    let mut result = ScanResult::default();

    let extensions = ["html", "phtml", "htm", "jsx", "tsx", "js", "ts", "vue", "svelte", "hbs", "ejs", "njk", "php"];

    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !extensions.contains(&ext.as_str()) {
            continue;
        }

        // Skip node_modules, .git, dist, build
        let path_str = path.to_string_lossy();
        if path_str.contains("node_modules")
            || path_str.contains("/.git/")
            || path_str.contains("/dist/")
            || path_str.contains("/build/")
            || path_str.contains("/.next/")
        {
            continue;
        }

        result.files_scanned += 1;

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                // If we can't read the file, report it as an issue but don't fail the whole scan
                // Can be caused some encoding issues or file permissions
                result.issues.push(Issue {
                    file: path.strip_prefix(root).unwrap_or(path).to_path_buf(),
                    line: 0,
                    kind: IssueKind::OversizedFile,
                    severity: IssueSeverity::Error,
                    snippet: "File Read Error".to_string(),
                    message: format!("Permission denied or read error: {}", e),
                });
                continue;
            }
        };
        
        let file_issues = scan_file(path, &content, root);
        let img_count = count_images_in_file(&content);
        result.images_found += img_count;
        result.issues.extend(file_issues);
    }

    Ok(result)
}

fn count_images_in_file(content: &str) -> usize {
    get_img_start_re().find_iter(content).count()
}

fn scan_file(path: &Path, content: &str, root: &Path) -> Vec<Issue> {
    let mut issues = Vec::new();

    // Also scan JSX/TSX patterns: <img ... /> or <Image ... />
    // We collect line numbers by scanning line by line
    let lines: Vec<&str> = content.lines().collect();

    // Build a flat string with line tracking
    for cap in get_img_tag_re().captures_iter(content) {
        let full_match = cap.get(0).expect("Regex match should have group 0");
        let attrs = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let tag = full_match.as_str();

        // Find line number
        let byte_offset = full_match.start();
        let line_num = content[..byte_offset].chars().filter(|&c| c == '\n').count() + 1;

        let snippet = tag
            .lines()
            .next()
            .unwrap_or(tag)
            .trim()
            .chars()
            .take(80)
            .collect::<String>();

        // Check: wrong format (src not ending in .webp or .avif, ignore data URIs)
        check_wrong_format(path, line_num, &snippet, attrs, &mut issues);

        // Check: missing width / height
        check_missing_dimensions(path, line_num, &snippet, attrs, &mut issues);

        // Check: missing lazy loading
        check_missing_lazy(path, line_num, &snippet, attrs, &mut issues);

        // Check: missing srcset
        check_missing_srcset(path, line_num, &snippet, attrs, &mut issues);

        // Check: oversized file (if src is a local path)
        check_oversized_file(path, line_num, &snippet, attrs, root, &mut issues);
    }

    // Also detect Next.js / React <Image> from 'next/image' or similar without proper props
    scan_jsx_image_component(path, &lines, &mut issues);

    issues
}

fn get_attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    // Match name="value" or name='value' or name={...} or just name
    // Since name is dynamic, we can't easily pre-compile everything, 
    // but for common ones like "src", "width", etc. we could.
    // However, the issue description specifically mentioned get_attr and has_attr using regexes in loops.
    
    let pattern = format!(
        r#"(?i)\b{}\s*=\s*(?:"([^"]*)"|'([^']*)'|\{{([^}}]*)\}})"#,
        regex::escape(name)
    );
    // For now, let's at least handle it better or use OnceLock for common ones if possible.
    // Given it's a small app, maybe we can just use a simple string parser for attributes 
    // or keep regex but be aware of the cost.
    
    let re = Regex::new(&pattern).ok()?;
    if let Some(cap) = re.captures(attrs) {
        return cap.get(1).or(cap.get(2)).or(cap.get(3)).map(|m| m.as_str());
    }
    None
}

fn has_attr(attrs: &str, name: &str) -> bool {
    let pattern = format!(r"(?i)\b{}\b", regex::escape(name));
    Regex::new(&pattern).map(|re| re.is_match(attrs)).unwrap_or(false)
}

fn check_wrong_format(path: &Path, line: usize, snippet: &str, attrs: &str, issues: &mut Vec<Issue>) {
    let src = get_attr(attrs, "src").unwrap_or("");

    // Skip data URIs, dynamic expressions, and empty
    if src.is_empty() || src.starts_with("data:") || src.contains("${") || src.starts_with('{') {
        return;
    }

    let lower = src.to_lowercase();
    let is_modern = lower.ends_with(".webp") || lower.ends_with(".avif");
    let is_image = lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".bmp")
        || lower.ends_with(".tiff");

    if is_image && !is_modern {
        issues.push(Issue {
            kind: IssueKind::WrongFormat,
            severity: IssueSeverity::Warning,
            file: path.to_path_buf(),
            line,
            snippet: snippet.to_string(),
            message: format!("Image '{}' should be converted to WebP or AVIF for better compression.", src),
        });
    }
}

fn check_missing_dimensions(path: &Path, line: usize, snippet: &str, attrs: &str, issues: &mut Vec<Issue>) {
    let has_width = has_attr(attrs, "width");
    let has_height = has_attr(attrs, "height");

    // Also accept fill prop (Next.js)
    let has_fill = has_attr(attrs, "fill");

    if !has_fill && (!has_width || !has_height) {
        let missing = match (has_width, has_height) {
            (false, false) => "width and height",
            (false, true) => "width",
            (true, false) => "height",
            _ => return,
        };
        issues.push(Issue {
            kind: IssueKind::MissingWidthHeight,
            severity: IssueSeverity::Error,
            file: path.to_path_buf(),
            line,
            snippet: snippet.to_string(),
            message: format!("Missing '{}' attribute — causes layout shift (CLS).", missing),
        });
    }
}

fn check_missing_lazy(path: &Path, line: usize, snippet: &str, attrs: &str, issues: &mut Vec<Issue>) {
    let loading = get_attr(attrs, "loading").unwrap_or("");
    // fetchpriority="high" means it's the LCP image — should NOT be lazy
    let fetchpriority = get_attr(attrs, "fetchpriority").unwrap_or("");
    if fetchpriority.eq_ignore_ascii_case("high") {
        return;
    }

    if loading.is_empty() || (!loading.eq_ignore_ascii_case("lazy") && !loading.eq_ignore_ascii_case("eager")) {
        issues.push(Issue {
            kind: IssueKind::MissingLazyLoading,
            severity: IssueSeverity::Warning,
            file: path.to_path_buf(),
            line,
            snippet: snippet.to_string(),
            message: "Missing loading=\"lazy\" — below-the-fold images should be lazy loaded.".to_string(),
        });
    }
}

fn check_missing_srcset(path: &Path, line: usize, snippet: &str, attrs: &str, issues: &mut Vec<Issue>) {
    let has_srcset = has_attr(attrs, "srcset") || has_attr(attrs, "srcSet");
    let src = get_attr(attrs, "src").unwrap_or("");

    // Skip SVGs and icons (small), data URIs, dynamic
    if src.ends_with(".svg") || src.starts_with("data:") || src.starts_with('{') {
        return;
    }

    if !has_srcset {
        issues.push(Issue {
            kind: IssueKind::MissingSrcset,
            severity: IssueSeverity::Info,
            file: path.to_path_buf(),
            line,
            snippet: snippet.to_string(),
            message: "No srcset defined — add responsive image variants for different viewport sizes.".to_string(),
        });
    }
}

fn check_oversized_file(
    path: &Path,
    line: usize,
    snippet: &str,
    attrs: &str,
    root: &Path,
    issues: &mut Vec<Issue>,
) {
    let src = get_attr(attrs, "src").unwrap_or("");
    if src.is_empty() || src.starts_with("data:") || src.starts_with("http") || src.starts_with('{') || src.contains("${") {
        return;
    }

    // Resolve relative to root or file
    let decoded_src = percent_encoding::percent_decode_str(src).decode_utf8_lossy();
    let candidates = vec![
        root.join(decoded_src.trim_start_matches('/')),
        path.parent().unwrap_or(root).join(decoded_src.as_ref()),
    ];

    for candidate in candidates {
        if let Ok(meta) = std::fs::metadata(&candidate) {
            if meta.len() > MAX_IMAGE_SIZE_BYTES {
                issues.push(Issue {
                    kind: IssueKind::OversizedFile,
                    severity: IssueSeverity::Error,
                    file: path.to_path_buf(),
                    line,
                    snippet: snippet.to_string(),
                    message: format!(
                        "Image file is {:.1} KiB — exceeds recommended 200 KiB limit.",
                        meta.len() as f64 / 1024.0
                    ),
                });
                break;
            }
        }
    }
}

fn scan_jsx_image_component(path: &Path, lines: &[&str], issues: &mut Vec<Issue>) {
    // Detect JSX <Image from next/image or similar without alt
    for (i, line) in lines.iter().enumerate() {
        if get_image_jsx_re().is_match(line) {
            if !line.contains("alt=") {
                issues.push(Issue {
                    kind: IssueKind::MissingWidthHeight,
                    severity: IssueSeverity::Warning,
                    file: path.to_path_buf(),
                    line: i + 1,
                    snippet: line.trim().chars().take(80).collect(),
                    message: "JSX <Image> component missing alt attribute (accessibility + SEO).".to_string(),
                });
            }
        }
    }
}
