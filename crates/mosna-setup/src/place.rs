//! Where the MOSNA folder goes, and moving it there.

use std::path::{Path, PathBuf};

/// The name the folder takes wherever it is put.
pub const FOLDER_NAME: &str = "MOSNA_Enhanced";

/// Whether `directory` is a MOSNA source folder.
pub fn is_mosna(directory: &Path) -> bool {
    directory.join("Cargo.toml").is_file() && directory.join("crates/mosna-install").is_dir()
}

/// Windows compares paths without regard to case.
fn same_path(a: &Path, b: &Path) -> bool {
    let normal = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/");
        text.trim_end_matches('/').to_lowercase()
    };
    normal(a) == normal(b)
}

fn is_inside(path: &Path, directory: &Path) -> bool {
    let normal = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
        format!("{}/", text.trim_end_matches('/'))
    };
    normal(path).starts_with(&normal(directory)) && !same_path(path, directory)
}

/// Where the folder goes by default: left alone when it already carries its
/// name, `MOSNA_Enhanced` in the home folder otherwise — a download lands as
/// `Downloads\MOSNA_Enhanced-main`, which is nowhere to keep it.
pub fn default_destination(source: &Path, home: &Path) -> PathBuf {
    if source.file_name().is_some_and(|name| name == FOLDER_NAME) {
        source.to_path_buf()
    } else {
        home.join(FOLDER_NAME)
    }
}

/// The destination for a folder picked in the dialog: the folder MOSNA goes
/// *into* — unless it is where MOSNA already is, or already carries its name.
pub fn destination_in(chosen: &Path, source: &Path) -> PathBuf {
    if same_path(chosen, source) || chosen.file_name().is_some_and(|name| name == FOLDER_NAME) {
        chosen.to_path_buf()
    } else {
        chosen.join(FOLDER_NAME)
    }
}

/// What putting the folder at a destination amounts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// It is there already.
    Stay,
    /// Nothing is there yet.
    Fresh,
    /// An earlier copy of MOSNA is there, and is brought up to date — results
    /// and configuration kept, MOSNA's own files replaced.
    Update,
}

/// Decide what putting `source` at `destination` means, or why it cannot be done.
pub fn check(source: &Path, destination: &Path) -> Result<Placement, String> {
    if destination.as_os_str().is_empty() || !destination.is_absolute() {
        return Err(
            "Indiquez le chemin complet du dossier, par exemple C:\\Users\\vous\\MOSNA_Enhanced."
                .into(),
        );
    }
    if same_path(source, destination) {
        return Ok(Placement::Stay);
    }
    if is_inside(destination, source) {
        return Err(format!(
            "Le dossier de destination ne peut pas être à l'intérieur du dossier actuel ({}).",
            source.display()
        ));
    }
    let occupied = std::fs::read_dir(destination)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if !occupied {
        return Ok(Placement::Fresh);
    }
    if is_mosna(destination) {
        return Ok(Placement::Update);
    }
    Err(format!(
        "{} existe déjà et contient autre chose que MOSNA. Choisissez un dossier vide ou inexistant.",
        destination.display()
    ))
}

/// Put the folder at `destination`, and return where it now is.
pub fn relocate(
    source: &Path,
    destination: &Path,
    placement: Placement,
) -> std::io::Result<PathBuf> {
    match placement {
        Placement::Stay => return Ok(source.to_path_buf()),
        Placement::Fresh => {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // An empty folder in the way would make the rename fail.
            let _ = std::fs::remove_dir(destination);
            // A rename is instant, and keeps a previous build's cache usable;
            // it only works within one drive.
            if std::fs::rename(source, destination).is_ok() {
                remove_empty_download_folder(source);
                return Ok(destination.to_path_buf());
            }
            copy_tree(source, destination)?;
        }
        Placement::Update => copy_tree(source, destination)?,
    }
    // The copy is complete, so the original is no longer needed. A file held
    // open elsewhere is not worth failing the install over.
    if is_mosna(source) && std::fs::remove_dir_all(source).is_ok() {
        remove_empty_download_folder(source);
    }
    Ok(destination.to_path_buf())
}

/// Windows' "Extract all" nests the folder in one of the same name, which
/// would be left behind empty.
fn remove_empty_download_folder(source: &Path) {
    if let Some(parent) = source.parent() {
        let ours = parent
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(FOLDER_NAME));
        if ours {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

/// Copy a directory tree, replacing files that are already there.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_mosna(directory: &Path) {
        std::fs::create_dir_all(directory.join("crates/mosna-install")).unwrap();
        std::fs::write(directory.join("Cargo.toml"), "[workspace]").unwrap();
        std::fs::write(directory.join("crates/mosna-install/lib.rs"), "x").unwrap();
    }

    #[test]
    fn a_picked_folder_receives_a_mosna_enhanced_folder() {
        let source = Path::new("/downloads/MOSNA_Enhanced-main");
        assert_eq!(
            destination_in(Path::new("/data"), source),
            Path::new("/data/MOSNA_Enhanced")
        );
        assert_eq!(
            destination_in(Path::new("/data/MOSNA_Enhanced"), source),
            Path::new("/data/MOSNA_Enhanced")
        );
        assert_eq!(destination_in(source, source), source);
    }

    #[test]
    fn the_default_leaves_a_well_named_folder_alone() {
        let home = Path::new("/home/me");
        assert_eq!(
            default_destination(Path::new("/downloads/MOSNA_Enhanced-main"), home),
            Path::new("/home/me/MOSNA_Enhanced")
        );
        assert_eq!(
            default_destination(Path::new("/tools/MOSNA_Enhanced"), home),
            Path::new("/tools/MOSNA_Enhanced")
        );
    }

    #[test]
    fn paths_compare_as_windows_does() {
        assert!(same_path(
            Path::new(r"C:\Users\Me\MOSNA"),
            Path::new(r"c:\users\me\mosna\")
        ));
        assert!(is_inside(Path::new(r"C:\A\B"), Path::new(r"c:\a")));
        assert!(!is_inside(Path::new(r"C:\AB"), Path::new(r"C:\A")));
    }

    #[test]
    fn a_destination_inside_the_source_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("MOSNA_Enhanced-main");
        fake_mosna(&source);
        assert!(check(&source, &source.join("inner")).is_err());
        assert_eq!(check(&source, &source), Ok(Placement::Stay));
    }

    #[test]
    fn a_folder_holding_something_else_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        fake_mosna(&source);
        let other = dir.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("thesis.docx"), "x").unwrap();
        assert!(check(&source, &other).is_err());
        assert!(other.join("thesis.docx").is_file());
    }

    #[test]
    fn a_fresh_destination_receives_the_folder_and_the_original_goes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("MOSNA_Enhanced-main/MOSNA_Enhanced-main");
        fake_mosna(&source);
        let destination = dir.path().join("home/MOSNA_Enhanced");

        let placement = check(&source, &destination).unwrap();
        assert_eq!(placement, Placement::Fresh);
        relocate(&source, &destination, placement).unwrap();

        assert!(is_mosna(&destination));
        assert!(!source.exists());
        assert!(
            !dir.path().join("MOSNA_Enhanced-main").exists(),
            "empty download folder left"
        );
    }

    #[test]
    fn an_update_keeps_what_the_user_added() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("new");
        fake_mosna(&source);
        std::fs::write(source.join("Cargo.toml"), "new").unwrap();
        let destination = dir.path().join("MOSNA_Enhanced");
        fake_mosna(&destination);
        std::fs::write(destination.join("results.csv"), "mine").unwrap();

        let placement = check(&source, &destination).unwrap();
        assert_eq!(placement, Placement::Update);
        relocate(&source, &destination, placement).unwrap();

        assert_eq!(
            std::fs::read_to_string(destination.join("Cargo.toml")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("results.csv")).unwrap(),
            "mine"
        );
        assert!(!source.exists());
    }
}
