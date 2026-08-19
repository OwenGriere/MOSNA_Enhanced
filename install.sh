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

# Rust is not the only thing this needs. A C compiler, for the few dependencies
# that carry native code; and Python, because the figures are drawn by the `xy`
# charting library, which the installer puts into an environment of its own
# under the prefix. Ask for both up front: finding out two minutes into a build
# is finding out too late.
if [ "$skip_build" = false ]; then
    missing=""
    command -v cc >/dev/null 2>&1 || missing="$missing a C compiler"

    python=""
    for candidate in python3 python; do
        if command -v "$candidate" >/dev/null 2>&1; then
            python="$candidate"
            break
        fi
    done
    if [ -z "$python" ]; then
        missing="$missing python3 (3.11 or newer)"
    fi

    if [ -n "$missing" ]; then
        cat >&2 <<MSG
error: this needs more than Rust, and this machine is missing:$missing

Install them with whichever of these fits your distribution:

    Debian, Ubuntu, Mint    sudo apt install build-essential python3 python3-venv
    Fedora, RHEL            sudo dnf install gcc python3
    Arch, Manjaro           sudo pacman -S base-devel python
    openSUSE                sudo zypper install gcc python3

then run this script again.
MSG
        exit 1
    fi

    echo "Building MOSNA (this takes a few minutes the first time)…"
    cargo build --release --bin mosna --bin mosna-gui
fi

# `--renderer` points at the Python package that draws the figures. The
# installer checks the interpreter's version, builds the environment and
# installs into it; the version rules live there, where they are tested, not in
# this script.
cargo run --release --quiet --bin mosna-install -- \
    --build-dir "$here/target/release" \
    --config "$here/CONFIG/configuration.yaml" \
    --icon "$here/assets/logo.ico" \
    --renderer "$here/python" \
    "$@"
