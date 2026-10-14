//! kiters-core — the vocabulary of a site and the hooks that run through
//! its making.
//!
//! Everything the pipeline knows how to say, it says with the types here;
//! every place the pipeline does anything, a hook can step in. A build is
//! a walk through these types with the hooks fired along the way:
//!
//! ```text
//! discover → front matter → markdown → target → html → page → build
//! ```

pub mod hooks;
pub mod site;

pub use hooks::{Hook, Hooks, DiscoverEvent, FrontMatterEvent, MarkdownEvent, TargetEvent, HtmlEvent, PageEvent, BuildEvent};
pub use site::*;
