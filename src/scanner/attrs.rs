/// Extract the value of a named HTML/JSX attribute from an attrs string.
/// Handles `name="val"`, `name='val'`, and `name={val}` forms.
/// Performs word-boundary checking so `src` won't match `data-src`.
pub fn get_attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let name_lower = name.to_lowercase();
    let attrs_lower = attrs.to_lowercase();
    let needle = format!("{}=", name_lower);

    let mut search_from = 0;
    while let Some(rel) = attrs_lower[search_from..].find(&needle) {
        let abs = search_from + rel;
        if is_word_start(attrs_lower.as_bytes(), abs) {
            let after_eq = abs + needle.len();
            let rest = &attrs[after_eq..];
            return match rest.as_bytes().first()? {
                b'"' => rest.get(1..)?.split('"').next(),
                b'\'' => rest.get(1..)?.split('\'').next(),
                b'{' => {
                    let end = rest.find('}')?;
                    rest.get(1..end)
                }
                _ => None,
            };
        }
        search_from = abs + 1;
    }
    None
}

/// Check whether a named attribute is present (word-boundary matched).
pub fn has_attr(attrs: &str, name: &str) -> bool {
    let needle = name.to_lowercase();
    let lower = attrs.to_lowercase();
    let mut pos = 0;
    while let Some(rel) = lower[pos..].find(&needle) {
        let abs = pos + rel;
        let end = abs + needle.len();
        if is_word_start(lower.as_bytes(), abs) && is_word_end(lower.as_bytes(), end) {
            return true;
        }
        pos = abs + 1;
    }
    false
}

#[inline]
fn is_word_start(bytes: &[u8], pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let b = bytes[pos - 1];
    !b.is_ascii_alphanumeric() && b != b'-' && b != b'_'
}

#[inline]
fn is_word_end(bytes: &[u8], pos: usize) -> bool {
    if pos >= bytes.len() {
        return true;
    }
    let b = bytes[pos];
    !b.is_ascii_alphanumeric() && b != b'-' && b != b'_'
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{get_attr, has_attr};

    #[test]
    fn get_attr_double_quote() {
        assert_eq!(get_attr(r#"src="hero.jpg" alt="test""#, "src"), Some("hero.jpg"));
    }

    #[test]
    fn get_attr_single_quote() {
        assert_eq!(get_attr("src='hero.jpg'", "src"), Some("hero.jpg"));
    }

    #[test]
    fn get_attr_jsx_braces() {
        assert_eq!(get_attr(r#"src={imgSrc}"#, "src"), Some("imgSrc"));
    }

    #[test]
    fn get_attr_case_insensitive() {
        assert_eq!(get_attr(r#"SRC="hero.jpg""#, "src"), Some("hero.jpg"));
    }

    #[test]
    fn get_attr_does_not_match_data_src() {
        // `src` should not match inside `data-src`
        assert_eq!(get_attr(r#"data-src="other.jpg""#, "src"), None);
    }

    #[test]
    fn get_attr_missing_returns_none() {
        assert_eq!(get_attr(r#"width="100""#, "src"), None);
    }

    #[test]
    fn has_attr_present() {
        assert!(has_attr(r#"width="100" height="200""#, "width"));
    }

    #[test]
    fn has_attr_word_boundary() {
        // `width` should NOT match inside `max-width`
        assert!(!has_attr(r#"max-width="100""#, "width"));
    }

    #[test]
    fn has_attr_standalone_boolean() {
        assert!(has_attr("loading lazy fill", "fill"));
    }

    #[test]
    fn has_attr_case_insensitive() {
        assert!(has_attr(r#"Loading="lazy""#, "loading"));
    }

    #[test]
    fn has_attr_not_present() {
        assert!(!has_attr(r#"src="img.jpg""#, "alt"));
    }
}
