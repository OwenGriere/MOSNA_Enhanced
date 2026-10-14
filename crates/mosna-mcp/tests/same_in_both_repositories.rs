//! The crate lives in MOSNA_Enhanced and in MOSNA_GUI, so that whoever
//! downloads only one of them gets the server. The two copies must not drift.
//!
//! When the other checkout sits beside this one (the usual layout), every file
//! of the crate is compared byte for byte. When it does not, there is nothing
//! to compare against and the test passes: a lone download is a normal case.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn files(root: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.insert(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    found
}

#[test]
fn both_copies_are_identical() {
    let this = Path::new(env!("CARGO_MANIFEST_DIR"));
    let this_canonical = std::fs::canonicalize(this).unwrap();
    // crates/mosna-mcp → workspace → the folder holding both checkouts.
    let Some(desk) = this.ancestors().nth(3) else {
        return;
    };

    let others: Vec<PathBuf> = ["MOSNA_Enhanced", "MOSNA_GUI"]
        .iter()
        .map(|name| desk.join(name).join("crates/mosna-mcp"))
        .filter(|path| path.is_dir())
        .filter(|path| std::fs::canonicalize(path).ok().as_ref() != Some(&this_canonical))
        .collect();
    let Some(other) = others.first() else {
        eprintln!("no other checkout beside this one: nothing to compare");
        return;
    };

    let (mine, theirs) = (files(this), files(other));
    assert_eq!(
        mine,
        theirs,
        "the two copies of mosna-mcp hold different files ({} and {})",
        this.display(),
        other.display()
    );
    let differing: Vec<&PathBuf> = mine
        .iter()
        .filter(|file| std::fs::read(this.join(file)).ok() != std::fs::read(other.join(file)).ok())
        .collect();
    assert!(
        differing.is_empty(),
        "mosna-mcp differs from {} in {differing:?}: copy the crate across",
        other.display()
    );
}
