#!/usr/bin/env bash
#
# Build MOSNA and install it for the current user.
#
#   ./install.sh                     install into ~/.local
#   ./install.sh --prefix /usr/local install system-wide (needs write access)
#   ./install.sh --dry-run           show what would happen
#   ./install.sh --uninstall         remove a previous install
#
# The real work is done by the `mosna-install` binary, which is tested; this
# script only builds and hands over, so there is no untested logic in bash.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

if ! command -v cargo >/dev/null 2>&1; then
    cat >&2 <<'MSG'
error: cargo was not found.

MOSNA is built with Rust. Install the toolchain with:

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

then open a new shell and run this script again.
MSG
    exit 1
fi

# --uninstall needs no build, and refusing to run without one would leave a
# user unable to clean up after a failed install.
skip_build=false
for argument in "$@"; do
    case "$argument" in
        --uninstall) skip_build=true ;;
    esac
done

# Rust is not the only thing the build needs. `freetype-sys` and
# `yeslogic-fontconfig-sys` resolve their libraries through pkg-config, so
# without the development files the build dies two minutes in with an error
# about PKG_CONFIG_PATH that names nothing anyone could install. Ask first.
if [ "$skip_build" = false ]; then
    missing=""
    command -v cc >/dev/null 2>&1 || missing="$missing a C compiler"
    command -v pkg-config >/dev/null 2>&1 || missing="$missing pkg-config"
    if command -v pkg-config >/dev/null 2>&1; then
        pkg-config --exists fontconfig || missing="$missing fontconfig"
    fi

    if [ -n "$missing" ]; then
        cat >&2 <<MSG
error: the build needs more than Rust, and this machine is missing:$missing

Install them with whichever of these fits your distribution:

    Debian, Ubuntu, Mint    sudo apt install build-essential pkg-config libfontconfig1-dev
    Fedora, RHEL            sudo dnf install gcc pkgconf-pkg-config fontconfig-devel
    Arch, Manjaro           sudo pacman -S base-devel pkgconf fontconfig
    openSUSE                sudo zypper install gcc pkg-config fontconfig-devel

then run this script again. (They are needed only to build; the figures the
analysis draws are rendered with fontconfig at run time.)
MSG
        exit 1
    fi

    echo "Building MOSNA (this takes a few minutes the first time)…"
    cargo build --release --bin mosna --bin mosna-gui
fi

cargo run --release --quiet --bin mosna-install -- \
    --build-dir "$here/target/release" \
    --config "$here/CONFIG/configuration.yaml" \
    --icon "$here/assets/logo.ico" \
    "$@"
