//! kiters-parse — turning a content tree into items, with the hooks
//! fired at every file.

mod content;

pub use content::{read_config, read_item, split_front_matter, walk_content, WalkError};
