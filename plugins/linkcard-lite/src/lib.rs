//! linkcard — the card transform of kite's linkcard plugin, said in
//! Rust on this side of the boundary, for the parity the two engines
//! owe each other. Three shapes of writing stand for a card:
//!
//! - a `card` code block, its lines saying the card's parts
//!   (`url: …`, `title: …`, `desc: …`, `size: small|regular|grid`, and
//!   their Chinese names too);
//! - a `<hyperlink-card href="…">` tag, the shape a post moved from
//!   Halo carries (an inline one is the small card);
//! - a paragraph that is one bare link and nothing else.
//!
//! What a card's writing leaves out the card asks for with
//! `data-need`, and card.js — inlined here with card.css, carried once
//! into any page that has a card — fills in from a favicon service
//! and, where the settings allow it, a link detail service, in the
//! reader's browser. Everything a card already says, nothing touches.

#![cfg(target_arch = "wasm32")]

wit_bindgen::generate!({ path: "../../crates/kiters-wasm/wit", world: "plugin" });

const CARD_CSS: &str = include_str!("../assets/card.css");
const CARD_JS: &str = include_str!("../assets/card.js");

struct LinkcardLite;

impl Guest for LinkcardLite {
    fn on_discover(_path: String, _kind: String) -> bool {
        false
    }

    fn on_front_matter(item: Item) -> Item {
        item
    }

    fn on_markdown(_url: String, body: String) -> String {
        body
    }

    fn on_target(target: Planned) -> Outcome {
        Outcome::Keep(target)
    }

    fn on_html(_url: String, html: String) -> String {
        fold_cards(&html)
    }

    fn on_page(page: Page) -> Page {
        page
    }

    fn on_build_complete(_report: Report) {}
}

export!(LinkcardLite);

/// Walks the HTML; every card-shaped thing becomes a card, and — when
/// at least one did — the page carries the stylesheet and the script
/// once, at head and body's end.
fn fold_cards(html: &str) -> String {
    let mut folded = fold_code_blocks(html);
    folded = fold_hyperlink_tags(&folded);
    folded = fold_bare_links(&folded);
    if folded.contains("class=\"kite-card\"") {
        dress_page(&folded)
    } else {
        folded
    }
}

/// A page that has a card carries what a card needs: the style at the
/// head's end, the settings and the script at the body's. A head or
/// body it does not have takes them at the end instead.
fn dress_page(html: &str) -> String {
    let style = format!("<style>{CARD_CSS}</style>");
    let script = format!(
        "<script>window.KiteLinkCard={{settings:{{target:\"_blank\",fetch:true}}}};</script>\n<script>{CARD_JS}</script>"
    );
    let with_style = match html.rfind("</head>") {
        Some(at) => format!("{}{}{}", &html[..at], style, &html[at..]),
        None => html.to_string(),
    };
    match with_style.rfind("</body>") {
        Some(at) => format!("{}{}{}", &with_style[..at], script, &with_style[at..]),
        None => format!("{with_style}{script}"),
    }
}

/// The `card` code blocks of the page, drawn. A block comrak renders
/// plainly, as `card` is no language it knows; what the block says is
/// escaped, and is read back before it is parsed.
fn fold_code_blocks(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find("<pre><code class=\"language-") {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        // The opening tag's `>` ends it — and `<pre>` carries one of
        // its own, so the search starts past the whole prefix.
        let prefix_len = open + "<pre><code class=\"language-".len();
        let Some(tag_end) = rest[prefix_len..].find('>').map(|i| prefix_len + i) else { break };
        let body_start = tag_end + 1;
        let Some(close) = rest[body_start..].find("</code></pre>") else {
            out.push_str(after);
            return out;
        };
        let language = rest[prefix_len..tag_end].trim_matches('"');
        let body = &rest[body_start..body_start + close];
        let whole = &rest[open..body_start + close + "</code></pre>".len()];

        if matches!(language, "card" | "linkcard" | "link-card") {
            let text = strip_tags(&unescape(body));
            let params = parse_params(&text);
            if let Some(card) = draw_card(&params, "regular") {
                out.push_str(&card);
            } else {
                out.push_str(whole);
            }
        } else {
            out.push_str(whole);
        }
        rest = &rest[body_start + close + "</code></pre>".len()..];
    }
    out.push_str(rest);
    out
}

/// The `<hyperlink-card>` tags a post moved from Halo carries — and
/// the escaped shape the same tags take when the renderer scrubs
/// unknown HTML, which is also theirs.
fn fold_hyperlink_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let (plain, escaped) = next_tag(rest);
        // Whichever tag comes first on the page, plain or escaped.
        let hit = match (plain, escaped) {
            (Some(p), Some(e)) => Some(if p.0 <= e.0 { p } else { e }),
            (found, none) => found.or(none),
        };
        let Some((at, whole, params, inline)) = hit else { break };
        out.push_str(&rest[..at]);
        let default_size = if inline { "small" } else { "regular" };
        if params.contains_key("url") {
            match draw_card(&params, default_size) {
                Some(card) => out.push_str(&card),
                None => out.push_str(&whole),
            }
        } else {
            out.push_str(&whole);
        }
        rest = &rest[at + whole.len()..];
    }
    out.push_str(rest);
    out
}

/// The next hyperlink tag in `html`, plain or escaped: where it
/// starts, its whole span, what it says, and whether it is the inline
/// one.
fn next_tag(html: &str) -> (Option<TagHit>, Option<TagHit>) {
    let plain = find_tag(html, "<hyperlink-", "</hyperlink");
    let escaped = find_tag(html, "&lt;hyperlink-", "&lt;/hyperlink");
    (plain, escaped)
}

type TagHit = (usize, String, Params, bool);

/// One form of the tag: looked for from the start, closed by its
/// closing shape, its attributes read. The plain tag closes with a
/// real `>`, the escaped one spells it `&gt;` — and `&gt;` holds no
/// `>` of its own.
fn find_tag(html: &str, open: &str, close: &str) -> Option<TagHit> {
    let at = html.find(open)?;
    let end = html[at..].find(close)?;
    let tail = &html[at..at + end];
    let after_close = &html[at + end..];
    let whole_end = match (after_close.find("&gt;"), after_close.find('>')) {
        (Some(amp), Some(plain)) => end + amp.min(plain) + if amp <= plain { 4 } else { 1 },
        (Some(amp), None) => end + amp + 4,
        (None, Some(plain)) => end + plain + 1,
        (None, None) => return None,
    };
    let whole = html[at..at + whole_end].to_string();
    let inline = tail.starts_with("<hyperlink-inline-") || tail.starts_with("&lt;hyperlink-inline-");
    let mut params = Params::new();
    for (name, value) in attrs_of(tail) {
        if let Some(key) = match name {
            "href" => Some("url"),
            "custom-title" => Some("title"),
            "custom-description" => Some("desc"),
            "custom-image" => Some("image"),
            "custom-icon" => Some("icon"),
            "target" => Some("target"),
            "theme" => Some("size"),
            _ => None,
        } {
            params.insert(key.to_string(), unescape(&value));
        }
    }
    Some((at, whole, params, inline))
}

/// A tag's attributes, as they were typed — plain or escaped the same.
fn attrs_of(tag: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let name: String = rest[..eq].chars().rev().take_while(|c| c.is_alphanumeric() || *c == '-').collect::<Vec<_>>().into_iter().rev().collect();
        let after = &rest[eq + 1..];
        let Some(close) = after.find('"') else { break };
        let value = unescape(&after[1..close]);
        if !name.is_empty() {
            attrs.push((name, value));
        }
        rest = &after[close + 1..];
    }
    attrs
}

/// A paragraph that is exactly one bare link becomes a card; the rest
/// of the page stays byte for byte.
fn fold_bare_links(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    while let Some(rel) = html[pos..].find("<p><a href=\"") {
        let p = pos + rel;
        out.push_str(&html[pos..p]);
        match bare_link_par(&html[p..]) {
            Some((href, len)) => {
                let mut params = Params::new();
                params.insert("url".to_string(), href.to_string());
                match draw_card(&params, "regular") {
                    Some(card) => out.push_str(&card),
                    None => out.push_str(&html[p..p + len]),
                }
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

/// What a card's block says, in the words it was typed: the tags
/// around and inside it are gone and what they escaped is back.
fn strip_tags(escaped_text: &str) -> String {
    let mut out = String::with_capacity(escaped_text.len());
    let mut rest = escaped_text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        match rest[open..].find('>') {
            Some(close) => rest = &rest[open + close + 1..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

type Params = std::collections::HashMap<String, String>;

/// A card's block says its parts: a key and value to a line, as
/// `title: …` or `key=value`, and a line that is only an address is
/// the address. Names read in Chinese the same as in English.
fn parse_params(text: &str) -> Params {
    let mut params = Params::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let said = line;
        if let Some((key, value)) = split_key_value(line) {
            if let Some(canonical) = name_of(&key.to_lowercase()) {
                params.insert(canonical.to_string(), unquote(value));
                continue;
            }
        }
        if !params.contains_key("url")
            && (line.starts_with("http://") || line.starts_with("https://") || line.starts_with('/'))
        {
            params.insert("url".to_string(), line.to_string());
        }
    }
    params
}

/// `key: value` or `key=value`, when the line says one.
fn split_key_value(line: &str) -> Option<(String, &str)> {
    let colon = line.find(':').or_else(|| line.find('='))?;
    let key = line[..colon].trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    Some((key.to_string(), line[colon + 1..].trim()))
}

/// A key's canonical name, in either of the card writer's languages.
fn name_of(key: &str) -> Option<&'static str> {
    Some(match key {
        "url" | "link" | "href" | "address" | "链接" | "网址" | "地址" | "链接地址" => "url",
        "title" | "name" | "标题" | "名称" => "title",
        "desc" | "description" | "summary" | "描述" | "简介" | "摘要" | "说明" => "desc",
        "image" | "img" | "cover" | "图片" | "封面" | "配图" => "image",
        "icon" | "favicon" | "logo" | "图标" | "站点图标" => "icon",
        "size" | "theme" | "尺寸" | "大小" => "size",
        "target" | "新窗口" | "打开方式" => "target",
        _ => return None,
    })
}

/// A value with a pair of quotes loses them.
fn unquote(value: &str) -> String {
    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

/// The card what a card's markup says stands for — the shape kite's
/// linkcard draws, so the two engines' cards read as one.
fn draw_card(params: &Params, default_size: &str) -> Option<String> {
    let address = params.get("url")?.trim();
    if address.is_empty() {
        return None;
    }
    let (scheme_ok, host, rest) = split_address(address)?;
    if !scheme_ok || (host.is_empty() && !rest.starts_with('/')) {
        return None;
    }

    let mut host_said = host.to_string();
    if host_said.is_empty() {
        host_said = address.trim_matches('/').rsplit('/').next().unwrap_or(address).to_string();
    }

    let mut size = match default_size {
        "small" | "regular" | "grid" => default_size,
        _ => "regular",
    }
    .to_string();
    if let Some(said) = params.get("size") {
        if matches!(said.as_str(), "small" | "regular" | "grid") {
            size = said.clone();
        }
    }
    let mut target = "_blank".to_string();
    match params.get("target").map(|s| s.as_str()) {
        Some("_self") | Some("当前窗口") | Some("本窗口") | Some("当前页") => target = "_self".to_string(),
        _ => {}
    }

    let mut needs = Vec::new();
    if !params.contains_key("icon") {
        needs.push("icon");
    }
    if !params.contains_key("title") {
        needs.push("title");
    }
    if !params.contains_key("desc") && size != "small" {
        needs.push("desc");
    }
    if size != "small" && !params.contains_key("image") {
        needs.push("image");
    }

    let mut card = format!(
        "<a class=\"kite-card\" data-size=\"{size}\" href=\"{}\"",
        escape(address)
    );
    if !target.is_empty() {
        card.push_str(&format!(" target=\"{target}\""));
        if target == "_blank" {
            card.push_str(" rel=\"noopener\"");
        }
    }
    if !needs.is_empty() {
        card.push_str(&format!(" data-need=\"{}\"", needs.join(" ")));
    }
    card.push('>');

    if size != "small" {
        if let Some(image) = params.get("image") {
            card.push_str(&format!(
                "<span class=\"kc-media\"><img src=\"{}\" alt=\"\" loading=\"lazy\"></span>",
                escape(image)
            ));
        } else {
            card.push_str("<span class=\"kc-media\" data-fill=\"image\"></span>");
        }
    }

    card.push_str("<span class=\"kc-body\">");
    if let Some(icon) = params.get("icon") {
        card.push_str(&format!(
            "<img class=\"kc-icon\" src=\"{}\" alt=\"\" width=\"32\" height=\"32\" loading=\"lazy\">",
            escape(icon)
        ));
    }
    card.push_str("<span class=\"kc-text\">");
    let (title, fill) = match params.get("title") {
        Some(said) => (escape(said), false),
        None => (escape(&host_said), true),
    };
    let fill_attr = if fill { " data-fill=\"title\"" } else { "" };
    card.push_str(&format!("<strong class=\"kc-title\"{fill_attr}>{title}</strong>"));
    if let Some(desc) = params.get("desc") {
        card.push_str(&format!("<span class=\"kc-desc\">{}</span>", escape(desc)));
    }
    if !host_said.is_empty() {
        card.push_str(&format!("<span class=\"kc-host\">{}</span>", escape(&host_said)));
    }
    card.push_str("</span></span></a>");
    Some(card)
}

/// An address in three: whether its scheme is one a card can point
/// at, its host, and all that follows the scheme.
fn split_address(address: &str) -> Option<(bool, &str, &str)> {
    if let Some(rest) = address.strip_prefix("https://") {
        let (host, rest) = split_host(rest);
        Some((true, host, rest))
    } else if let Some(rest) = address.strip_prefix("http://") {
        let (host, rest) = split_host(rest);
        Some((true, host, rest))
    } else if address.starts_with('/') {
        Some((true, "", address))
    } else {
        Some((false, "", address))
    }
}

fn split_host(rest: &str) -> (&str, &str) {
    match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, ""),
    }
}

/// What goes into the card's HTML, said safely: the ampersands,
/// angles and quotes of the card's own writing kept from meaning.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// What the renderer escaped, for the card's parts to read again.
fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
