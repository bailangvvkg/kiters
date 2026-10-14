//! kiters-parse — turning a content tree into items, with the hooks
//! fired at every file.

mod content;

pub use content::{read_config, read_item, walk_content, WalkError};
