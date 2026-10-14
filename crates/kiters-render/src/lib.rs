//! kiters-render — markdown to HTML, HTML into a page.

mod highlight;
mod layout;
mod markdown;
mod shortcode;

pub use highlight::{highlight_code_blocks, Highlighter};
pub use layout::Layout;
pub use markdown::{markdown_to_html, RenderOptions};
pub use shortcode::fold_shortcodes;
