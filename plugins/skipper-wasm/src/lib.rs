//! skipper — takes pages out of the site: any target whose address
//! holds `byebye` never stands.

#![cfg(target_arch = "wasm32")]

wit_bindgen::generate!({ path: "../../crates/kiters-wasm/wit", world: "plugin" });

struct Skipper;

impl Guest for Skipper {
    fn on_target(target: Planned) -> Outcome {
        if target.url.contains("byebye") {
            Outcome::Skip
        } else {
            Outcome::Keep(target)
        }
    }

    fn on_discover(_path: String, _kind: String) -> bool {
        // a file of prose that never becomes an item
        false
    }
}

export!(Skipper);
