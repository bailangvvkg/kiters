//! skipper — takes pages out of the site: any target whose address
//! holds `byebye` never stands.

#![cfg(target_arch = "wasm32")]

wit_bindgen::generate!({ path: "../../crates/kiters-wasm/wit", world: "plugin" });

struct Skipper;

impl Guest for Skipper {
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
        if target.url.contains("byebye") {
            Outcome::Skip
        } else {
            Outcome::Keep(target)
        }
    }

    fn on_html(_url: String, html: String) -> String {
        html
    }

    fn on_page(page: Page) -> Page {
        page
    }

    fn on_build_complete(_report: Report) {}
}

export!(Skipper);
