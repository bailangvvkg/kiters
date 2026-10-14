//! Markdown to HTML, the one way the whole site does it.

use comrak::{markdown_to_html as comrak_to_html, Options};

/// How markdown becomes HTML.
#[derive(Clone, Copy, Default)]
pub struct RenderOptions {
    /// Unsafe HTML in the prose is passed through rather than scrubbed.
    ///
    /// The site's own files are the author's own words; this is the
    /// switch that trusts them.
    pub unsafe_html: bool,
    /// Code blocks wear syntect color.
    pub highlight: bool,
}

/// Renders markdown to HTML.
///
/// Code blocks carry their language as a class — `language-rust` — for
/// the page to color however its theme colors; the renderer itself has
/// no opinion about colors. GFM is on: tables, task lists, footnotes,
/// autolinks, strikethrough.
pub fn markdown_to_html(source: &str, opts: RenderOptions) -> String {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.render.unsafe_ = opts.unsafe_html;
    comrak_to_html(source, &options)
}
