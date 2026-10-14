//! stamp — the smallest plugin that proves the whole road: every page
//! it is handed comes back carrying its mark.
//!
//! The moments it does not care about return what they were given,
//! unchanged — the whole of a plugin's obligation to the rest.

#![cfg(target_arch = "wasm32")]

wit_bindgen::generate!({ path: "../../crates/kiters-wasm/wit", world: "plugin" });

struct Stamp;

impl Guest for Stamp {
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
        html
    }

    fn on_page(mut page: Page) -> Page {
        page.html.push_str("\n<!-- stamped by stamp-wasm -->\n");
        page
    }

    fn on_build_complete(_report: Report) {}
}

export!(Stamp);
