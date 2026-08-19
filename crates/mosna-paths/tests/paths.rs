//! Tests of path resolution, written before the implementation.
//!
//! Once the application is installed rather than run from the repository, it
//! has to find three things whose location is no longer obvious: its
//! configuration file, the `mosna` binary the interface launches, and its
//! assets. Each has a precedence order, and getting it wrong means an installed
//! copy silently reads the developer's configuration — or none at all.

use std::path::{Path, PathBuf};

use mosna_paths::{binary, config_file, layout::Layout, Environment};

/// An environment with nothing set, so each test states exactly what it relies
/// on rather than inheriting the developer's machine.
fn bare(exe_dir: &Path) -> Environment {
    Environment {
        exe_dir: Some(exe_dir.to_path_buf()),
        home: None,
        xdg_config_home: None,
        xdg_data_home: None,
        mosna_config: None,
        mosna_bin: None,
        mosna_python: None,
        current_dir: None,
        desktop_dir: None,
        user_profile: None,
        app_data: None,
        local_app_data: None,
    }
}

// ---------------------------------------------------------------------------
// The configuration file
// ---------------------------------------------------------------------------

/// An explicit path always wins: it is what the user typed.
#[test]
fn an_explicit_path_takes_precedence_over_everything() {
    let dir = tempfile::tempdir().unwrap();
    let explicit = dir.path().join("mine.yaml");
    std::fs::write(&explicit, "Tysserand: {}\n").unwrap();

    let mut environment = bare(dir.path());
    environment.mosna_config = Some(dir.path().join("from-env.yaml"));

    assert_eq!(
        config_file::resolve(Some(&explicit), &environment),
        explicit
    );
}

/// `MOSNA_CONFIG` comes next, so a run can be pointed at another configuration
/// without touching the command line — which is how a scheduler or a container
/// would drive it.
#[test]
fn the_environment_variable_comes_second() {
    let dir = tempfile::tempdir().unwrap();
    let from_env = dir.path().join("from-env.yaml");
    std::fs::write(&from_env, "Tysserand: {}\n").unwrap();

    let mut environment = bare(dir.path());
    environment.mosna_config = Some(from_env.clone());

    assert_eq!(config_file::resolve(None, &environment), from_env);
}

/// Then the user's own copy under the XDG configuration directory. This is the
/// one an installed application edits, so that saving from the interface never
/// writes into a system directory the user may not own.
#[test]
fn the_user_configuration_comes_third() {
    let dir = tempfile::tempdir().unwrap();
    let user = dir.path().join("config/mosna/configuration.yaml");
    std::fs::create_dir_all(user.parent().unwrap()).unwrap();
    std::fs::write(&user, "Tysserand: {}\n").unwrap();

    let mut environment = bare(dir.path());
    environment.xdg_config_home = Some(dir.path().join("config"));

    assert_eq!(config_file::resolve(None, &environment), user);
}

/// `XDG_CONFIG_HOME` is often unset; `~/.config` is the documented fallback.
#[test]
fn the_home_directory_backs_the_xdg_variable() {
    let dir = tempfile::tempdir().unwrap();
    let user = dir.path().join(".config/mosna/configuration.yaml");
    std::fs::create_dir_all(user.parent().unwrap()).unwrap();
    std::fs::write(&user, "Tysserand: {}\n").unwrap();

    let mut environment = bare(dir.path());
    environment.home = Some(dir.path().to_path_buf());

    assert_eq!(config_file::resolve(None, &environment), user);
}

/// Then the copy the installer laid down next to the binaries, which is what a
/// first run finds before the user has a configuration of their own.
#[test]
fn the_installed_copy_comes_fourth() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let installed = dir.path().join("share/mosna/configuration.yaml");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::fs::write(&installed, "Tysserand: {}\n").unwrap();

    let environment = bare(&bin);
    assert_eq!(config_file::resolve(None, &environment), installed);
}

/// Finally the repository layout, so `cargo run` from a checkout still works.
#[test]
fn the_repository_layout_is_the_last_resort() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("CONFIG/configuration.yaml");
    std::fs::create_dir_all(repo.parent().unwrap()).unwrap();
    std::fs::write(&repo, "Tysserand: {}\n").unwrap();

    let mut environment = bare(dir.path());
    environment.current_dir = Some(dir.path().to_path_buf());

    assert_eq!(config_file::resolve(None, &environment), repo);
}

/// With nothing to find, the resolver still returns the path a first run should
/// create, rather than failing: the interface opens, reports that it could not
/// load a configuration, and the user picks one.
#[test]
fn nothing_found_still_yields_a_usable_path() {
    let dir = tempfile::tempdir().unwrap();
    let mut environment = bare(dir.path());
    environment.home = Some(dir.path().to_path_buf());

    let resolved = config_file::resolve(None, &environment);
    assert_eq!(
        resolved,
        dir.path().join(".config/mosna/configuration.yaml"),
        "the fallback must be where the user's own copy belongs"
    );
}

/// Where a first run should copy the shipped configuration to.
#[test]
fn the_user_configuration_path_is_reported_independently() {
    let dir = tempfile::tempdir().unwrap();
    let mut environment = bare(dir.path());
    environment.xdg_config_home = Some(dir.path().join("cfg"));

    assert_eq!(
        config_file::user_path(&environment),
        Some(dir.path().join("cfg/mosna/configuration.yaml"))
    );
}

// ---------------------------------------------------------------------------
// The analysis binary
// ---------------------------------------------------------------------------

/// `MOSNA_BIN` wins, so a developer can point the interface at a debug build.
#[test]
fn the_binary_can_be_overridden_by_the_environment() {
    let dir = tempfile::tempdir().unwrap();
    let custom = dir.path().join("my-mosna");
    std::fs::write(&custom, b"").unwrap();

    let mut environment = bare(dir.path());
    environment.mosna_bin = Some(custom.clone());

    assert_eq!(binary::resolve_analysis(&environment), custom);
}

/// Otherwise the copy beside the interface, which is how the installer lays
/// them out and what makes a relocated install work.
#[test]
fn the_binary_beside_the_interface_is_preferred() {
    let dir = tempfile::tempdir().unwrap();
    let beside = dir.path().join(binary::ANALYSIS_FILE_NAME);
    std::fs::write(&beside, b"").unwrap();

    let environment = bare(dir.path());
    assert_eq!(binary::resolve_analysis(&environment), beside);
}

/// With no neighbour, the bare name is returned so the shell searches `PATH`.
#[test]
fn the_bare_name_lets_the_shell_search_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let environment = bare(dir.path());
    assert_eq!(
        binary::resolve_analysis(&environment),
        PathBuf::from(binary::ANALYSIS_FILE_NAME)
    );
}

// ---------------------------------------------------------------------------
// The install layout
// ---------------------------------------------------------------------------

/// A user install goes under `~/.local`, which needs no administrator rights —
/// the default, because asking for `sudo` to try an application is a poor
/// trade.
#[test]
fn the_default_prefix_is_the_user_local_directory() {
    let dir = tempfile::tempdir().unwrap();
    let mut environment = bare(dir.path());
    environment.home = Some(dir.path().to_path_buf());

    assert_eq!(
        Layout::default_prefix(&environment),
        Some(dir.path().join(".local"))
    );
}

#[test]
fn the_layout_follows_the_filesystem_hierarchy_standard() {
    let layout = Layout::new(Path::new("/opt/mosna"));

    assert_eq!(layout.bin_dir(), Path::new("/opt/mosna/bin"));
    assert_eq!(
        layout.analysis_binary(),
        Path::new("/opt/mosna/bin").join(binary::ANALYSIS_FILE_NAME)
    );
    assert_eq!(
        layout.interface_binary(),
        Path::new("/opt/mosna/bin").join(binary::INTERFACE_FILE_NAME)
    );
    assert_eq!(
        layout.shipped_config(),
        Path::new("/opt/mosna/share/mosna/configuration.yaml")
    );
    assert_eq!(
        layout.menu_entry().unwrap(),
        Path::new("/opt/mosna/share/applications/mosna.desktop")
    );
    assert_eq!(
        layout.icon(),
        Path::new("/opt/mosna/share/icons/hicolor/256x256/apps/mosna.png")
    );
}

/// Every path the installer writes must sit under the prefix; anything else
/// would make an uninstall unable to clean up after itself.
#[test]
fn every_installed_path_is_inside_the_prefix() {
    let prefix = Path::new("/opt/mosna");
    let layout = Layout::new(prefix);

    for path in layout.all_paths() {
        assert!(
            path.starts_with(prefix),
            "{} escapes the prefix",
            path.display()
        );
    }
}

/// An install must know everything it wrote, so it can be undone.
#[test]
fn the_layout_enumerates_what_an_install_creates() {
    let layout = Layout::new(Path::new("/opt/mosna"));
    let paths = layout.all_paths();

    assert!(paths.contains(&layout.analysis_binary()));
    assert!(paths.contains(&layout.interface_binary()));
    assert!(paths.contains(&layout.menu_entry().unwrap()));
    assert!(paths.contains(&layout.icon()));
    assert!(paths.contains(&layout.shipped_config()));
}
