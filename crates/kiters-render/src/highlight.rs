//! Code blocks, wearing color.
//!
//! The renderer hands over `<pre><code class="language-x">` — plain,
//! escaped, honest. Here each such block becomes syntax-highlighted
//! HTML: the block's language looked up in syntect's set, the colors
//! said once by a theme, every byte the renderer escaped unescaped
//! for the highlighting and its output carrying its own escaping.
//!
//! Blocks without a language, and languages the set does not know,
//! stay exactly as they were.

use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// The colors, looked up once and kept for the build's life.
pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

impl Highlighter {
    /// The built-in languages and a light theme that reads on paper
    /// and on screens.
    pub fn new() -> Self {
        Highlighter {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            theme: ThemeSet::load_defaults().themes["InspiredGitHub"].clone(),
        }
    }

    /// One block of already-escaped code, said in color. `None` when
    /// the language is not known: the block is not ours to change.
    pub fn block(&self, language: &str, escaped: &str) -> Option<String> {
        let syntax = self.syntaxes.find_syntax_by_token(language)?;
        let code = unescape(escaped);
        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut html = String::from("<pre class=\"code code-");
        html.push_str(language);
        html.push_str("\">");
        for line in code.split_inclusive('
') {
            let ranges = highlighter.highlight_line(line, &self.syntaxes).ok()?;
            let said = syntect::html::styled_line_to_highlighted_html(&ranges[..], syntect::html::IncludeBackground::No).ok()?;
            html.push_str(&said);
        }
        html.push_str("</pre>");
        Some(html)
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

/// Walks rendered HTML and dresses every `<pre><code
/// class="language-x">` block that can be dressed.
pub fn highlight_code_blocks(html: &str, highlighter: &Highlighter) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find("<pre><code") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open..];
        let Some(class_end) = after_open.find('>') else { break };
        let head = &after_open[..class_end];
        let body_start = class_end + 1;
        let Some(close) = after_open[body_start..].find("</code></pre>") else {
            out.push_str(after_open);
            return out;
        };
        let body = &after_open[body_start..body_start + close];
        let tail = &after_open[body_start + close + 13..];

        // `<pre><code class="language-rust">` says its language; a bare
        // `<pre><code>` says none and stays as it is.
        let language = head
            .strip_prefix("<pre><code class=\"language-")
            .and_then(|rest| rest.strip_suffix('"'));
        match language {
            Some(language) => match highlighter.block(language, body) {
                Some(dressed) => out.push_str(&dressed),
                None => {
                    out.push_str(&rest[open..open + body_start + close + 13]);
                }
            },
            None => out.push_str(&rest[open..open + body_start + close + 13]),
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// What the renderer escaped, for the highlighting to read again.
fn unescape(escaped: &str) -> String {
    escaped
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rust_block_wears_color() {
        let highlighter = Highlighter::new();
        let html = "<p>正文</p>\n<pre><code class=\"language-rust\">fn main() {}\n</code></pre>\n<p>后文</p>";
        let dressed = highlight_code_blocks(html, &highlighter);
        assert!(dressed.contains("<pre class=\"code code-rust\">"), "{dressed}");
        assert!(dressed.contains("span style"), "no color came out: {dressed}");
        assert!(dressed.starts_with("<p>正文</p>"), "the prose around was touched: {dressed}");
        assert!(dressed.ends_with("<p>后文</p>"), "the tail was lost: {dressed}");
    }

    #[test]
    fn a_bare_block_stays_as_it_is() {
        let highlighter = Highlighter::new();
        let html = "<pre><code>plain text\n</code></pre>";
        assert_eq!(highlight_code_blocks(html, &highlighter), html);
    }

    #[test]
    fn an_unknown_language_stays() {
        let highlighter = Highlighter::new();
        let html = "<pre><code class=\"language-nosuchlang\">x\n</code></pre>";
        assert_eq!(highlight_code_blocks(html, &highlighter), html);
    }
}
