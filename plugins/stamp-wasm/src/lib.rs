//! stamp — the smallest plugin that proves the whole road: every page
//! it is handed comes back carrying its mark.

#![cfg(target_arch = "wasm32")]

wit_bindgen::generate!({ path: "../../crates/kiters-wasm/wit", world: "plugin" });

struct Stamp;

impl Guest for Stamp {
    fn on_page(page: Page) -> Page {
        let mut page = page;
        page.html.push_str("\n<!-- stamped by stamp-wasm -->\n");
        page
    }

    fn on_build_complete(report: Report) {
        // Nothing to say that anyone reads; kept to prove the moment
        // crosses the boundary.
        let _ = report;
    }
}

export!(Stamp);
