//! The desktop entry that puts MOSNA in the application menu.

use mosna_paths::layout::{Layout, DESKTOP_ID};

/// Build the `.desktop` file for an install at `layout`.
///
/// `Exec` is absolute: the menu launches entries from an unspecified working
/// directory, so a relative command would find nothing.
///
/// `Categories` names exactly one main category — `Science` — with `Biology`
/// as its refinement. Listing two main categories makes the entry appear twice
/// in the application menu, which `desktop-file-validate` warns about.
///
/// `StartupWMClass` matches the window title the interface sets, which is what
/// lets the desktop associate the running window with this entry — without it
/// the taskbar shows a second, icon-less item.
pub fn entry(layout: &Layout) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=MOSNA\n\
         GenericName=Spatial omics analysis\n\
         Comment=Spatial network construction and analysis for spatial omics\n\
         Exec={exec}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Science;Biology;\n\
         Keywords=spatial;omics;network;niche;assortativity;\n\
         StartupNotify=true\n\
         StartupWMClass=MOSNA Graphic Interface\n",
        exec = layout.interface_binary().display(),
        icon = DESKTOP_ID,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_comes_first() {
        let entry = entry(&Layout::new("/opt/mosna"));
        assert!(entry.starts_with("[Desktop Entry]\n"));
    }

    #[test]
    fn the_command_is_absolute() {
        let entry = entry(&Layout::new("/opt/mosna"));
        let exec = entry
            .lines()
            .find(|line| line.starts_with("Exec="))
            .unwrap()
            .trim_start_matches("Exec=");
        assert!(
            std::path::Path::new(exec).is_absolute(),
            "Exec is relative: {exec}"
        );
    }

    #[test]
    fn the_icon_is_named_not_pathed() {
        // A bare name lets the theme pick the right size; a path pins one.
        let entry = entry(&Layout::new("/opt/mosna"));
        assert!(entry.contains(&format!("Icon={DESKTOP_ID}\n")));
        assert!(!entry.contains("Icon=/"));
    }

    #[test]
    fn the_categories_end_with_a_semicolon() {
        // The specification requires the list separator after the last item.
        let entry = entry(&Layout::new("/opt/mosna"));
        let categories = entry
            .lines()
            .find(|line| line.starts_with("Categories="))
            .unwrap();
        assert!(categories.ends_with(';'), "{categories}");
    }

    #[test]
    fn the_prefix_reaches_the_command() {
        let entry = entry(&Layout::new("/home/someone/.local"));
        assert!(entry.contains("Exec=/home/someone/.local/bin/mosna-gui"));
    }
}
