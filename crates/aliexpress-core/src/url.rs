//! Query string encoding, kept here rather than pulled in as a dependency: a
//! single function is all that is needed.

/// Encodes `text` for a query string value: `application/x-www-form-urlencoded`,
/// with a space as `+`.
pub fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Turns a keyword into the slug AliExpress puts in the search page path
/// (`/w/wholesale-<slug>.html`). The path is cosmetic -- the `SearchText`
/// parameter is what is searched -- but it has to be a valid path segment.
pub fn path_slug(keyword: &str) -> String {
    let slug: String = keyword
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    let mut collapsed = String::with_capacity(slug.len());
    for c in slug.chars() {
        if c == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(c);
    }
    if collapsed.is_empty() {
        "search".to_string()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_encode_keeps_unreserved_and_escapes_the_rest() {
        // Arrange
        let text = "usb c ケーブル/a&b";
        // Act
        let encoded = form_encode(text);
        // Assert
        assert_eq!(
            encoded,
            "usb+c+%E3%82%B1%E3%83%BC%E3%83%96%E3%83%AB%2Fa%26b"
        );
    }

    #[test]
    fn path_slug_is_ascii_and_never_empty() {
        // Arrange / Act / Assert
        assert_eq!(path_slug("USB C  cable "), "usb-c-cable");
        assert_eq!(path_slug("ケーブル"), "search");
        assert_eq!(path_slug("iphone 15/16"), "iphone-15-16");
    }
}
