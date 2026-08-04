//! Figures shipped inside the binary.
//!
//! The manual is read from an installed copy, which may sit anywhere and has no
//! repository next to it. A figure loaded from a relative path would therefore
//! be missing exactly when it is needed, so the few images the manual uses are
//! compiled in.
//!
//! Only the ones it *uses*: each entry here is close to a megabyte carried by
//! every installed copy, and the test suite refuses one that no page shows.

/// Every figure compiled into the binary.
///
/// Listed rather than only matched, so the tests can compare the list against
/// what the manual actually shows — in both directions.
pub const EMBEDDED: &[&str] = &["images/GUI.png", "images/workflow.png"];

/// The bytes of a figure, by the path the manual refers to it by.
pub fn image(asset: &str) -> Option<&'static [u8]> {
    match asset {
        "images/GUI.png" => Some(include_bytes!("../../../../assets/images/GUI.png")),
        "images/workflow.png" => Some(include_bytes!("../../../../assets/images/workflow.png")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_figures_are_pngs() {
        for asset in EMBEDDED {
            let bytes = image(asset).unwrap_or_else(|| panic!("`{asset}` is listed but absent"));
            assert!(bytes.starts_with(b"\x89PNG"), "{asset} is not a PNG");
        }
    }

    /// The list and the match arms state the same thing twice, and must not
    /// drift apart.
    #[test]
    fn nothing_is_reachable_that_the_list_omits() {
        assert!(image("images/architecture.png").is_none());
        assert!(image("images/nowhere.png").is_none());
    }
}
