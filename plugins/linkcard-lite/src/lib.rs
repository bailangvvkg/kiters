//! linkcard-lite — the card transform of kite's linkcard plugin, said
//! in Rust on this side of the boundary, for the parity the two
//! engines owe each other: a paragraph that is one bare link becomes
//! one card, and nothing else changes.
//!
//! The shape matches kite's linkcard `draw`: host shown, card linked,
//! link opened safely — `target="_blank" rel="noopener"`.

#![cfg(target_arch = "wasm32")]

wit_bindgen::generate!({ path: "../../crates/kiters-wasm/wit", world: "plugin" });

struct LinkcardLite;

impl Guest for LinkcardLite {
    fn on_html(_url: String, html: String) -> String {
        fold_bare_links(&html)
    }
}

/// Walks the HTML; a paragraph that is exactly one bare link becomes a
/// card, and everything else stays byte for byte.
fn fold_bare_links(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    while let Some(rel) = html[pos..].find("<p>") {
        let p = pos + rel;
        out.push_str(&html[pos..p]);
        match bare_link_par(&html[p..]) {
            Some((href, len)) => {
                out.push_str(&card(href));
                pos = p + len;
            }
            None => {
                out.push_str("<p>");
                pos = p + 3;
            }
        }
    }
    out.push_str(&html[pos..]);
    out
}

/// `p` starts at `<p>`; `Some((href, whole))` when the paragraph is
/// exactly one bare link — the link's text is the link itself, and
/// nothing else is in the paragraph. `whole` is how many bytes of `p`
/// the paragraph takes.
fn bare_link_par(p: &str) -> Option<(&str, usize)> {
    let a = p.strip_prefix("<p><a href=\"")?;
    let href_end = a.find('"')?;
    let href = &a[..href_end];
    let after = &a[href_end + 1..];
    let tag_end = after.find('>')?;
    let text = &after[tag_end + 1..];
    let close = text.find("</a>")?;
    if &text[..close] != href {
        return None;
    }
    let tail = &text[close + 4..];
    let end = tail.strip_prefix("</p>")?;
    Some((href, p.len() - end.len()))
}

/// The card a bare link becomes: the host shown, the card linked, the
/// link opened safely.
fn card(href: &str) -> String {
    let host = host_of(href);
    format!(
        "<div class=\"linkcard\"><a class=\"lc-card\" href=\"{href}\" target=\"_blank\" rel=\"noopener\"><span class=\"lc-host\">{host}</span></a></div>"
    )
}

/// The authority part of the address — `host` or `host:port`, the part
/// a reader recognizes a site by.
fn host_of(href: &str) -> &str {
    let rest = href.strip_prefix("https://").or_else(|| href.strip_prefix("http://")).unwrap_or(href);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}
