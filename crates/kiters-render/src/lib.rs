//! kiters-render — markdown to HTML, HTML into a page.

mod layout;
mod markdown;
mod shortcode;

pub use layout::Layout;
pub use markdown::{markdown_to_html, RenderOptions};
pub use shortcode::fold_shortcodes;
