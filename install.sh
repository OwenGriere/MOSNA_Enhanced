#!/usr/bin/env bash
#
# Install MOSNA for the current user.
#
#   ./install.sh                     install into ~/.local
#   ./install.sh --no-figures        skip the Python environment (smallest install)
#   ./install.sh --prefix /usr/local install system-wide (needs write access)
#   ./install.sh --dry-run           show what would happen
#   ./install.sh --uninstall         remove a previous install
#
# The real work is done by the `mosna-install` binary, which is tested; this
# script only finds the binaries and hands over, so there is no untested logic
# in bash.
#
# It runs in two situations and tells them apart on its own: next to
# already-built binaries, as in a downloaded release, where nothing is compiled
# and no toolchain is needed; or in a source checkout, where the binaries are
# built first.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

# A downloaded release carries the binaries beside this script; a checkout does
# not. Which one this is decides everything below, so it is settled first.
if [ -x "$here/mosna-install" ]; then
    packaged=true
    build_dir="$here"
else
    packaged=false
    build_dir="$here/target/release"
fi

# --uninstall needs no build, and refusing to run without one would leave a
# user unable to clean up after a failed install. --no-figures is ours: it is
# removed from the arguments before the installer sees them, because what it
# means to the installer is simply not being given a renderer.
skip_build=false
figures=true
installing=true
arguments=()
for argument in "$@"; do
    case "$argument" in
        --uninstall) skip_build=true; installing=false; arguments+=("$argument") ;;
        --dry-run | --help | -h | --version | -V) installing=false; arguments+=("$argument") ;;
        --no-figures) figures=false ;;
        *) arguments+=("$argument") ;;
    esac
done
if [ "$packaged" = true ]; then
    skip_build=true
fi

if [ "$skip_build" = false ] && ! command -v cargo >/dev/null 2>&1; then
    cat >&2 <<'MSG'
error: cargo was not found, and there are no pre-built binaries beside this script.

The quickest way in is not to build at all — the release page carries binaries:

    https://github.com/OwenGriere/MOSNA_Enhanced/releases

To build from source instead, install the Rust toolchain with:

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

then open a new shell and run this script again.
MSG
    exit 1
fi

# Rust is not the only thing a build needs: a C compiler, for the few
# dependencies that carry native code. Python is separate — it is needed at run
# time to draw the figures, and only if figures are wanted at all. Ask for both
# up front: finding out two minutes into a build is finding out too late.
missing=""
if [ "$skip_build" = false ]; then
    command -v cc >/dev/null 2>&1 || missing="$missing a C compiler"
fi

python=""
if [ "$figures" = true ] && [ -d "$here/python" ]; then
    for candidate in python3 python; do
        if command -v "$candidate" >/dev/null 2>&1; then
            python="$candidate"
            break
        fi
    done
    if [ -z "$python" ]; then
        missing="$missing python3 (3.11 or newer)"
    fi
fi

if [ -n "$missing" ]; then
    cat >&2 <<MSG
error: this needs more than the sources, and this machine is missing:$missing

Install them with whichever of these fits your distribution:

    Debian, Ubuntu, Mint    sudo apt install build-essential python3 python3-venv
    Fedora, RHEL            sudo dnf install gcc python3
    Arch, Manjaro           sudo pacman -S base-devel python
    openSUSE                sudo zypper install gcc python3

then run this script again. To install without the figures — the analyses still
run and still write their tables — pass --no-figures instead.
MSG
    exit 1
fi

if [ "$skip_build" = false ]; then
    echo "Building MOSNA (this takes a few minutes the first time)…"
    cargo build --release --bin mosna --bin mosna-gui --bin mosna-install
fi

# `--renderer` points at the Python package that draws the figures. The
# installer checks the interpreter's version, builds the environment and
# installs into it; the version rules live there, where they are tested, not in
# this script. Left out entirely when figures are not wanted, which is what
# makes that the smaller install.
renderer=()
if [ "$figures" = true ] && [ -d "$here/python" ]; then
    renderer=(--renderer "$here/python")
fi

common=(
    --build-dir "$build_dir"
    --config "$here/CONFIG/configuration.yaml"
    --icon "$here/assets/logo.ico"
)

if [ "$packaged" = true ]; then
    "$here/mosna-install" "${common[@]}" "${renderer[@]}" "${arguments[@]}"
else
    cargo run --release --quiet --bin mosna-install -- \
        "${common[@]}" "${renderer[@]}" "${arguments[@]}"
fi

# Once MOSNA is installed, the Windows installer and uninstaller are of no use
# in this folder.
# INSTALLATION.exe does the same the other way round.
if [ "$installing" = true ]; then
    for name in INSTALLATION.exe UNINSTALL.exe; do
        if [ -f "$here/$name" ]; then
            rm -f "$here/$name"
            echo "removed $name (Windows only)"
        fi
    done
fi
