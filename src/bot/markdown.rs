/// Convert common markdown to Telegram-compatible HTML.
pub fn md_to_html(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut preserved: Vec<(String, String)> = Vec::new();

    let mut working = text.to_string();
    working = extract_fenced_blocks(&working, &mut preserved);
    working = extract_inline_code(&working, &mut preserved);
    working = html_escape(&working);
    working = convert_links(&working);
    working = convert_bold(&working);
    working = convert_italic(&working);
    working = convert_strikethrough(&working);
    working = convert_headers(&working);
    working = convert_lists(&working);

    for (marker, replacement) in preserved.iter().rev() {
        working = working.replace(marker, replacement);
    }

    working
}

fn extract_fenced_blocks(text: &str, preserved: &mut Vec<(String, String)>) -> String {
    let mut result = String::new();
    let mut pos = 0;
    let mut idx = 0u64;

    while let Some(start) = text[pos..].find("```") {
        let abs_start = pos + start;
        result.push_str(&text[pos..abs_start]);

        let after_fence = abs_start + 3;
        let code_start = match text[after_fence..].find('\n') {
            Some(nl) => after_fence + nl + 1,
            None => after_fence,
        };

        match text[code_start..].find("```") {
            Some(end) => {
                let code = &text[code_start..code_start + end];
                pos = code_start + end + 3;

                idx += 1;
                let marker = format!("\x01C{idx}\x01");
                preserved.push((
                    marker.clone(),
                    format!("<pre>{}</pre>", html_escape(code.trim())),
                ));
                result.push_str(&marker);
            }
            None => {
                result.push_str("```");
                pos = after_fence;
            }
        }
    }

    result.push_str(&text[pos..]);
    result
}

fn extract_inline_code(text: &str, preserved: &mut Vec<(String, String)>) -> String {
    let mut result = String::new();
    let mut pos = 0;
    let mut idx = 100u64;

    while let Some(start) = text[pos..].find('`') {
        result.push_str(&text[pos..pos + start]);
        pos += start + 1;

        match text[pos..].find('`') {
            Some(end) => {
                let code = &text[pos..pos + end];
                pos += end + 1;

                idx += 1;
                let marker = format!("\x01C{idx}\x01");
                preserved.push((
                    marker.clone(),
                    format!("<code>{}</code>", html_escape(code)),
                ));
                result.push_str(&marker);
            }
            None => {
                result.push('`');
                break;
            }
        }
    }

    result.push_str(&text[pos..]);
    result
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn convert_links(text: &str) -> String {
    let mut result = String::new();
    let mut pos = 0;

    while let Some(start) = text[pos..].find('[') {
        result.push_str(&text[pos..pos + start]);
        pos += start + 1;

        if let Some(bracket_end) = text[pos..].find(']') {
            let link_text = &text[pos..pos + bracket_end];
            pos += bracket_end + 1;

            if text[pos..].starts_with('(') {
                pos += 1;
                if let Some(paren_end) = text[pos..].find(')') {
                    let url = &text[pos..pos + paren_end];
                    pos += paren_end + 1;
                    result.push_str(&format!("<a href=\"{url}\">{}</a>", html_escape(link_text)));
                    continue;
                }
            }

            result.push('[');
            result.push_str(link_text);
            result.push(']');
            continue;
        }

        result.push('[');
    }

    result.push_str(&text[pos..]);
    result
}

fn convert_bold(text: &str) -> String {
    // Handle ***bold italic*** first, then **bold**
    let mut result = String::new();
    let mut pos = 0;

    while let Some(start) = text[pos..].find("***") {
        result.push_str(&text[pos..pos + start]);
        pos += start + 3;

        if let Some(end) = text[pos..].find("***") {
            let inner = &text[pos..pos + end];
            pos += end + 3;
            result.push_str(&format!("<b><i>{}</i></b>", inner));
            continue;
        }

        result.push_str("***");
    }

    result.push_str(&text[pos..]);
    let text = result;
    let mut result = String::new();
    pos = 0;

    while let Some(start) = text[pos..].find("**") {
        result.push_str(&text[pos..pos + start]);
        pos += start + 2;

        if let Some(end) = text[pos..].find("**") {
            let inner = &text[pos..pos + end];
            pos += end + 2;
            result.push_str(&format!("<b>{}</b>", inner));
            continue;
        }

        result.push_str("**");
    }

    result.push_str(&text[pos..]);
    result
}

fn convert_italic(text: &str) -> String {
    let mut result = text.to_string();

    for &delim in &['*', '_'] {
        let mut new_result = String::new();
        let mut pos = 0;

        while let Some(start) = result[pos..].find(delim) {
            new_result.push_str(&result[pos..pos + start]);
            pos += start + 1;

            if let Some(end) = result[pos..].find(delim) {
                let inner = &result[pos..pos + end];
                pos += end + 1;
                new_result.push_str(&format!("<i>{}</i>", inner));
                continue;
            }

            new_result.push(delim);
        }

        new_result.push_str(&result[pos..]);
        result = new_result;
    }

    result
}

fn convert_strikethrough(text: &str) -> String {
    let mut result = String::new();
    let mut pos = 0;

    while let Some(start) = text[pos..].find("~~") {
        result.push_str(&text[pos..pos + start]);
        pos += start + 2;

        if let Some(end) = text[pos..].find("~~") {
            let inner = &text[pos..pos + end];
            pos += end + 2;
            result.push_str(&format!("<s>{}</s>", inner));
            continue;
        }

        result.push_str("~~");
    }

    result.push_str(&text[pos..]);
    result
}

fn convert_headers(text: &str) -> String {
    let mut result = String::new();
    for line in text.lines() {
        if !result.is_empty() {
            result.push('\n');
        }
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("### ") {
            result.push_str(&format!("<b>{}</b>", rest.trim()));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            result.push_str(&format!("<b>{}</b>", rest.trim()));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            result.push_str(&format!("<b>{}</b>", rest.trim()));
        } else {
            result.push_str(line);
        }
    }
    result
}

fn convert_lists(text: &str) -> String {
    let mut result = String::new();
    for line in text.lines() {
        if !result.is_empty() {
            result.push('\n');
        }
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("- ") {
            result.push_str(&format!("• {}", rest));
        } else if let Some(rest) = trimmed.strip_prefix("* ") {
            result.push_str(&format!("• {}", rest));
        } else if let Some(rest) = trimmed.strip_prefix("+ ") {
            result.push_str(&format!("• {}", rest));
        } else {
            result.push_str(line);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_md_to_html() {
        assert_eq!(md_to_html(""), "");
    }

    #[test]
    fn bold_md_to_html() {
        assert_eq!(md_to_html("**hello**"), "<b>hello</b>");
    }

    #[test]
    fn italic_md_to_html() {
        assert_eq!(md_to_html("*hello*"), "<i>hello</i>");
        assert_eq!(md_to_html("_hello_"), "<i>hello</i>");
    }

    #[test]
    fn strikethrough_md_to_html() {
        assert_eq!(md_to_html("~~hello~~"), "<s>hello</s>");
    }

    #[test]
    fn inline_code_md_to_html() {
        assert_eq!(md_to_html("`code`"), "<code>code</code>");
    }

    #[test]
    fn fenced_code_md_to_html() {
        assert_eq!(
            md_to_html("```rust\nlet x = 1;\n```"),
            "<pre>let x = 1;</pre>"
        );
    }

    #[test]
    fn link_md_to_html() {
        assert_eq!(
            md_to_html("[click](https://example.com)"),
            "<a href=\"https://example.com\">click</a>"
        );
    }

    #[test]
    fn list_md_to_html() {
        assert_eq!(md_to_html("- item"), "• item");
        assert_eq!(md_to_html("* item"), "• item");
        assert_eq!(md_to_html("+ item"), "• item");
    }

    #[test]
    fn html_escaping() {
        assert_eq!(md_to_html("a & b"), "a &amp; b");
        assert_eq!(md_to_html("1 < 2"), "1 &lt; 2");
        assert_eq!(md_to_html("3 > 2"), "3 &gt; 2");
    }

    #[test]
    fn bold_italic_md_to_html() {
        assert_eq!(md_to_html("***both***"), "<b><i>both</i></b>");
    }
}
