use std::path::Path;

use super::attrs::{get_attr, has_attr};
use super::types::{Issue, IssueKind, IssueSeverity};

const MAX_IMAGE_SIZE_BYTES: u64 = 200 * 1024; // 200 KiB

// ─── Individual checkers ─────────────────────────────────────────────────────

pub fn check_wrong_format(
    path: &Path,
    line: usize,
    snippet: &str,
    attrs: &str,
    issues: &mut Vec<Issue>,
) {
    let src = get_attr(attrs, "src").unwrap_or("");
    if src.is_empty() || src.starts_with("data:") || src.contains("${") || src.starts_with('{') {
        return;
    }
    let lower = src.to_lowercase();
    let is_modern = lower.ends_with(".webp") || lower.ends_with(".avif");
    let is_legacy = lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".bmp")
        || lower.ends_with(".tiff");

    if is_legacy && !is_modern {
        issues.push(Issue {
            kind: IssueKind::WrongFormat,
            severity: IssueSeverity::Warning,
            file: path.to_path_buf(),
            line,
            snippet: snippet.to_string(),
            message: format!(
                "Image '{}' should be converted to WebP or AVIF for better compression.",
                src
            ),
        });
    }
}

pub fn check_missing_dimensions(
    path: &Path,
    line: usize,
    snippet: &str,
    attrs: &str,
    tag: &str,
    issues: &mut Vec<Issue>,
) {
    let has_fill = has_attr(attrs, "fill");
    if has_fill {
        return;
    }

    // If the tag contains PHP/template expressions, the regex attrs capture
    // may be incomplete (e.g. ternaries break `[^?]*`). Fall back to a raw
    // substring search on the full tag text for these two attribute names.
    let tag_lower = tag.to_lowercase();
    let has_template = tag_lower.contains("<?");

    let has_width  = has_attr(attrs, "width")
        || (has_template && tag_lower.contains("width"));
    let has_height = has_attr(attrs, "height")
        || (has_template && tag_lower.contains("height"));

    let missing = match (has_width, has_height) {
        (false, false) => "width and height",
        (false, true)  => "width",
        (true,  false) => "height",
        (true,  true)  => return,
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

pub fn check_missing_lazy(
    path: &Path,
    line: usize,
    snippet: &str,
    attrs: &str,
    issues: &mut Vec<Issue>,
) {
    // LCP images must NOT be lazy loaded
    if get_attr(attrs, "fetchpriority")
        .map(|v| v.eq_ignore_ascii_case("high"))
        .unwrap_or(false)
    {
        return;
    }

    let loading = get_attr(attrs, "loading").unwrap_or("");
    let has_valid_loading = loading.eq_ignore_ascii_case("lazy")
        || loading.eq_ignore_ascii_case("eager");

    if !has_valid_loading {
        issues.push(Issue {
            kind: IssueKind::MissingLazyLoading,
            severity: IssueSeverity::Warning,
            file: path.to_path_buf(),
            line,
            snippet: snippet.to_string(),
            message: "Missing loading=\"lazy\" — below-the-fold images should be lazy loaded."
                .to_string(),
        });
    }
}

pub fn check_missing_srcset(
    path: &Path,
    line: usize,
    snippet: &str,
    attrs: &str,
    issues: &mut Vec<Issue>,
) {
    let src = get_attr(attrs, "src").unwrap_or("");
    // SVGs, data URIs and dynamic expressions don't benefit from srcset
    if src.ends_with(".svg") || src.starts_with("data:") || src.starts_with('{') {
        return;
    }

    let has_srcset = has_attr(attrs, "srcset") || has_attr(attrs, "srcSet");
    if !has_srcset {
        issues.push(Issue {
            kind: IssueKind::MissingSrcset,
            severity: IssueSeverity::Info,
            file: path.to_path_buf(),
            line,
            snippet: snippet.to_string(),
            message: "No srcset defined — add responsive image variants for different viewport sizes."
                .to_string(),
        });
    }
}

pub fn check_oversized_file(
    path: &Path,
    line: usize,
    snippet: &str,
    attrs: &str,
    root: &Path,
    issues: &mut Vec<Issue>,
) {
    let src = get_attr(attrs, "src").unwrap_or("");
    if src.is_empty()
        || src.starts_with("data:")
        || src.starts_with("http")
        || src.starts_with('{')
        || src.contains("${")
    {
        return;
    }

    let decoded = percent_encoding::percent_decode_str(src).decode_utf8_lossy();
    let candidates = [
        root.join(decoded.trim_start_matches('/')),
        path.parent().unwrap_or(root).join(decoded.as_ref()),
    ];

    for candidate in &candidates {
        if let Ok(meta) = std::fs::metadata(candidate) {
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        check_missing_dimensions, check_missing_lazy, check_missing_srcset,
        check_wrong_format,
    };
    use crate::scanner::types::{IssueKind, IssueSeverity};
    use std::path::Path;

    fn path() -> &'static Path {
        Path::new("test.html")
    }

    // ── check_wrong_format ──────────────────────────────────────────────────

    #[test]
    fn wrong_format_png_raises_warning() {
        let mut issues = vec![];
        check_wrong_format(path(), 1, "<img>", r#"src="hero.png""#, &mut issues);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::WrongFormat);
        assert_eq!(issues[0].severity, IssueSeverity::Warning);
    }

    #[test]
    fn wrong_format_webp_no_issue() {
        let mut issues = vec![];
        check_wrong_format(path(), 1, "<img>", r#"src="hero.webp""#, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn wrong_format_avif_no_issue() {
        let mut issues = vec![];
        check_wrong_format(path(), 1, "<img>", r#"src="hero.avif""#, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn wrong_format_data_uri_skipped() {
        let mut issues = vec![];
        check_wrong_format(path(), 1, "<img>", r#"src="data:image/png;base64,abc""#, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn wrong_format_dynamic_src_skipped() {
        let mut issues = vec![];
        check_wrong_format(path(), 1, "<img>", r#"src="${imgUrl}""#, &mut issues);
        assert!(issues.is_empty());
    }

    // ── check_missing_dimensions ────────────────────────────────────────────

    #[test]
    fn missing_both_dimensions() {
        let mut issues = vec![];
        check_missing_dimensions(path(), 1, "<img>", r#"src="a.jpg""#, r#"<img src="a.jpg">"#, &mut issues);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("width and height"));
    }

    #[test]
    fn missing_only_height() {
        let mut issues = vec![];
        check_missing_dimensions(path(), 1, "<img>", r#"src="a.jpg" width="100""#, r#"<img src="a.jpg" width="100">"#, &mut issues);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("height"));
    }

    #[test]
    fn has_both_dimensions_no_issue() {
        let mut issues = vec![];
        check_missing_dimensions(
            path(), 1, "<img>",
            r#"src="a.jpg" width="100" height="50""#,
            r#"<img src="a.jpg" width="100" height="50">"#,
            &mut issues,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn php_ternary_width_height_no_issue() {
        // PHP ternary expressions like <?= $w ? 'width="..."' : 'width="1"' ?>
        // break the attrs capture group — must check the full tag text.
        let tag = r#"<img src="logo.png" <?= $w ? 'width="80"' : 'width="1"' ?> <?= $h ? 'height="30"' : 'height="1"' ?>/>"#;
        let mut issues = vec![];
        check_missing_dimensions(path(), 1, tag, "", tag, &mut issues);
        assert!(issues.is_empty(), "unexpected issues: {:?}", issues);
    }

    #[test]
    fn fill_prop_skips_dimension_check() {
        let mut issues = vec![];
        check_missing_dimensions(path(), 1, "<img>", r#"fill src="a.jpg""#, r#"<img fill src="a.jpg">"#, &mut issues);
        assert!(issues.is_empty());
    }

    // ── check_missing_lazy ──────────────────────────────────────────────────

    #[test]
    fn missing_lazy_raises_warning() {
        let mut issues = vec![];
        check_missing_lazy(path(), 1, "<img>", r#"src="a.jpg""#, &mut issues);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::MissingLazyLoading);
    }

    #[test]
    fn lazy_loading_no_issue() {
        let mut issues = vec![];
        check_missing_lazy(path(), 1, "<img>", r#"src="a.jpg" loading="lazy""#, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn eager_loading_no_issue() {
        let mut issues = vec![];
        check_missing_lazy(path(), 1, "<img>", r#"src="a.jpg" loading="eager""#, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn fetchpriority_high_skips_lazy_check() {
        let mut issues = vec![];
        check_missing_lazy(
            path(), 1, "<img>",
            r#"src="a.jpg" fetchpriority="high""#,
            &mut issues,
        );
        assert!(issues.is_empty());
    }

    // ── check_missing_srcset ────────────────────────────────────────────────

    #[test]
    fn missing_srcset_raises_info() {
        let mut issues = vec![];
        check_missing_srcset(path(), 1, "<img>", r#"src="a.jpg""#, &mut issues);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::MissingSrcset);
        assert_eq!(issues[0].severity, IssueSeverity::Info);
    }

    #[test]
    fn srcset_present_no_issue() {
        let mut issues = vec![];
        check_missing_srcset(
            path(), 1, "<img>",
            r#"src="a.jpg" srcset="a-2x.jpg 2x""#,
            &mut issues,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn svg_skips_srcset_check() {
        let mut issues = vec![];
        check_missing_srcset(path(), 1, "<img>", r#"src="icon.svg""#, &mut issues);
        assert!(issues.is_empty());
    }
}
