pub mod attrs;
pub mod checks;
pub mod types;

pub use types::{Issue, IssueKind, IssueSeverity, ScanResult};

use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;
use walkdir::WalkDir;

use checks::{
    check_missing_dimensions, check_missing_lazy, check_missing_srcset, check_oversized_file,
    check_wrong_format,
};

// ─── Static regexes ──────────────────────────────────────────────────────────

static IMG_TAG_RE: OnceLock<Regex> = OnceLock::new();
static IMG_START_RE: OnceLock<Regex> = OnceLock::new();
static IMAGE_JSX_RE: OnceLock<Regex> = OnceLock::new();

fn img_tag_re() -> &'static Regex {
    IMG_TAG_RE.get_or_init(|| {
        Regex::new(
            r#"(?is)<img\b((?:"[^"]*"|'[^']*'|<\?(?:php|=)[^?]*\?>|[^>"'])*)(?:\s*/?>|>)"#,
        )
        .expect("invalid IMG_TAG_RE")
    })
}

fn img_start_re() -> &'static Regex {
    IMG_START_RE.get_or_init(|| Regex::new(r"(?i)<img\b").expect("invalid IMG_START_RE"))
}

fn image_jsx_re() -> &'static Regex {
    IMAGE_JSX_RE.get_or_init(|| Regex::new(r"(?i)<Image\b").expect("invalid IMAGE_JSX_RE"))
}

// ─── Skip rules ──────────────────────────────────────────────────────────────

const SKIP_SEGMENTS: &[&str] = &[
    "node_modules",
    "/.git/",
    "/dist/",
    "/build/",
    "/.next/",
];

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "html", "phtml", "htm", "jsx", "tsx", "js", "ts", "vue", "svelte", "hbs", "ejs", "njk",
    "php",
];

fn should_skip(path: &Path) -> bool {
    let s = path.to_string_lossy();
    SKIP_SEGMENTS.iter().any(|seg| s.contains(seg))
}

// ─── Public API ──────────────────────────────────────────────────────────────

pub fn scan_directory(root: &Path) -> Result<ScanResult> {
    let mut result = ScanResult::default();

    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        if should_skip(path) {
            continue;
        }

        result.files_scanned += 1;

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
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

        result.images_found += count_img_tags(&content);
        result.issues.extend(scan_file(path, &content, root));
    }

    Ok(result)
}

// ─── Internal ────────────────────────────────────────────────────────────────

fn count_img_tags(content: &str) -> usize {
    img_start_re().find_iter(content).count()
}

pub(crate) fn scan_file(path: &Path, content: &str, root: &Path) -> Vec<Issue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for cap in img_tag_re().captures_iter(content) {
        let full = cap.get(0).expect("group 0 always present");
        let attrs = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let tag = full.as_str();

        let line_num = content[..full.start()]
            .chars()
            .filter(|&c| c == '\n')
            .count()
            + 1;

        // Capture up to 5 lines of the tag so the detail view shows real context.
        let snippet: String = {
            let tag_lines: Vec<&str> = tag.lines().collect();
            let capped = tag_lines.iter().take(5);
            let joined = capped.cloned().collect::<Vec<_>>().join("\n");
            // Hard cap at 300 chars to avoid overflowing the UI widget.
            if joined.len() > 300 { joined[..300].to_string() } else { joined }
        };

        check_wrong_format(path, line_num, &snippet, attrs, &mut issues);
        check_missing_dimensions(path, line_num, &snippet, attrs, tag, &mut issues);
        check_missing_lazy(path, line_num, &snippet, attrs, &mut issues);
        check_missing_srcset(path, line_num, &snippet, attrs, &mut issues);
        check_oversized_file(path, line_num, &snippet, attrs, root, &mut issues);
    }

    scan_jsx_image_component(path, &lines, &mut issues);
    issues
}

fn scan_jsx_image_component(path: &Path, lines: &[&str], issues: &mut Vec<Issue>) {
    let mut i = 0;
    while i < lines.len() {
        if image_jsx_re().is_match(lines[i]) {
            let mut tag_buf = String::new();
            let mut j = i;
            while j < lines.len() {
                tag_buf.push_str(lines[j]);
                tag_buf.push('\n');
                if lines[j].contains("/>") || lines[j].contains("</Image>") {
                    break;
                }
                j += 1;
            }

            if !tag_buf.contains("alt=") {
                issues.push(Issue {
                    kind: IssueKind::MissingAlt,
                    severity: IssueSeverity::Warning,
                    file: path.to_path_buf(),
                    line: i + 1,
                    snippet: lines[i].trim().chars().take(80).collect(),
                    message: "JSX <Image> component missing alt attribute (accessibility + SEO)."
                        .to_string(),
                });
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
}

// ─── Integration tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{scan_file, Issue, IssueKind};
    use std::path::Path;

    fn scan(html: &str) -> Vec<Issue> {
        let root = Path::new(".");
        scan_file(Path::new("test.html"), html, root)
    }

    #[test]
    fn detects_multiple_issues_on_bare_img() {
        // A bare <img> with a jpg src should trigger: WrongFormat, MissingWidthHeight,
        // MissingLazyLoading, MissingSrcset
        let issues = scan(r#"<img src="hero.jpg">"#);
        let kinds: Vec<_> = issues.iter().map(|i| &i.kind).collect();
        assert!(kinds.contains(&&IssueKind::WrongFormat));
        assert!(kinds.contains(&&IssueKind::MissingWidthHeight));
        assert!(kinds.contains(&&IssueKind::MissingLazyLoading));
        assert!(kinds.contains(&&IssueKind::MissingSrcset));
    }

    #[test]
    fn clean_img_tag_has_no_issues() {
        let issues = scan(
            r#"<img src="hero.webp" width="800" height="400" loading="lazy" srcset="hero-2x.webp 2x">"#,
        );
        assert!(issues.is_empty(), "unexpected issues: {:?}", issues);
    }

    #[test]
    fn jsx_image_without_alt_detected() {
        let issues = scan("<Image src=\"hero.webp\" width={800} height={400} />");
        let kinds: Vec<_> = issues.iter().map(|i| &i.kind).collect();
        assert!(kinds.contains(&&IssueKind::MissingAlt));
    }

    #[test]
    fn jsx_image_with_alt_no_alt_issue() {
        let issues = scan(r#"<Image src="hero.webp" alt="A hero" width={800} height={400} />"#);
        let alt_issues: Vec<_> = issues
            .iter()
            .filter(|i| i.kind == IssueKind::MissingAlt)
            .collect();
        assert!(alt_issues.is_empty());
    }

    #[test]
    fn line_numbers_are_correct() {
        let html = "<!-- line 1 -->\n<!-- line 2 -->\n<img src=\"hero.jpg\">";
        let issues = scan(html);
        assert!(issues.iter().all(|i| i.line == 3));
    }
}
