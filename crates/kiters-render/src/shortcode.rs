//! Shortcodes: the little structures markdown does not have.
//!
//! A shortcode is `{{< kind title >}}` … `{{< /kind >}}` around a body,
//! and each kind folds into a markdown shape before rendering — a note
//! becomes a blockquote with a bold head — so nothing here depends on
//! raw HTML being let through, and the rendering is one way for every
//! site.
//!
//! Kinds: `note`, `info`, `tip`, `warn` — same shape, different marker.

/// Folds every shortcode in the markdown into plain markdown.
///
/// An opening marker without its closing one is left as it is: prose
/// that reads oddly is better than prose that silently vanishes.
pub fn fold_shortcodes(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some((before, kind, title, body, after)) = split_one(rest) {
        out.push_str(before);
        out.push_str(&fold(kind, title, body));
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The first shortcode in `text`, if there is one: what came before it,
/// its kind, its title, its body, and what comes after.
fn split_one(text: &str) -> Option<(&str, &str, &str, &str, &str)> {
    let open = text.find("{{<")?;
    let head_end = text[open..].find(">}}")? + open;
    let head = &text[open + 3..head_end];
    let mut parts = head.trim().splitn(2, char::is_whitespace);
    let kind = parts.next()?.trim();
    let title = parts.next().unwrap_or("").trim();
    if kind.starts_with('/') {
        return None; // a stray closing marker, not ours to fold
    }
    let close = format!("{{{{< /{kind} >}}}}");
    let body_start = head_end + 3;
    let close_at = text[body_start..].find(&close)? + body_start;
    Some((
        &text[..open],
        kind,
        title,
        &text[body_start..close_at],
        &text[close_at + close.len()..],
    ))
}

/// One kind's shape: the body as a blockquote under a bold head.
fn fold(kind: &str, title: &str, body: &str) -> String {
    let marker = match kind {
        "note" => "Note",
        "info" => "Info",
        "tip" => "Tip",
        "warn" => "Warn",
        other => other,
    };
    let head = if title.is_empty() { marker.to_string() } else { format!("{marker}: {title}") };
    let mut folded = format!("> **{head}**\n");
    for line in body.trim().lines() {
        if line.trim().is_empty() {
            folded.push_str(">\n");
        } else {
            folded.push_str("> ");
            folded.push_str(line);
            folded.push('\n');
        }
    }
    folded.push('\n');
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_folds_into_a_blockquote() {
        let src = "before\n\n{{< note 注意 >}}\n里面是**正文**\n{{< /note >}}\n\nafter";
        let folded = fold_shortcodes(src);
        assert!(folded.contains("> **Note: 注意**"), "{folded}");
        assert!(folded.contains("> 里面是**正文**"), "{folded}");
        assert!(folded.starts_with("before"));
        assert!(folded.ends_with("after"));
    }

    #[test]
    fn an_unmatched_marker_stays() {
        let src = "{{< note 没 >}}\n正文";
        assert_eq!(fold_shortcodes(src), src);
    }

    #[test]
    fn two_shortcodes_fold() {
        let src = "{{< tip >}}一{{< /tip >}}\n\n中\n\n{{< warn 警 >}}二{{< /warn >}}";
        let folded = fold_shortcodes(src);
        assert!(folded.contains("> **Tip**"));
        assert!(folded.contains("> **Warn: 警**"));
        assert!(folded.contains("中"));
    }
}
