//! kiters-render — markdown to HTML, HTML into a page.

mod layout;
mod markdown;

pub use layout::Layout;
pub use markdown::{markdown_to_html, RenderOptions};
