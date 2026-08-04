# MOSNA

Spatial network construction and analysis for spatial omics: a native
application that reconstructs a spatial network per sample, measures which cell
types sit next to which, and groups neighbourhoods into spatial niches.

This is the Rust implementation, and the root of the project. It replaces the
earlier Python/PySide6 version, keeping the same configuration file, the same
output layout and the same three-step workflow — an existing
`CONFIG/configuration.yaml` works unchanged.

## Install

No Python, no conda, no scientific stack. What it does need, to build:

* the **Rust toolchain** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
* a **C compiler**, **pkg-config** and the **fontconfig** development files,
  which the font libraries behind the figures link against:

  | | |
  |---|---|
  | Debian, Ubuntu, Mint | `sudo apt install build-essential pkg-config libfontconfig1-dev` |
  | Fedora, RHEL | `sudo dnf install gcc pkgconf-pkg-config fontconfig-devel` |
  | Arch, Manjaro | `sudo pacman -S base-devel pkgconf fontconfig` |
  | openSUSE | `sudo zypper install gcc pkg-config fontconfig-devel` |

`install.sh` checks for all of this before it starts building, and names the
command to run if something is missing. The graphical interface additionally
needs a running desktop session — X11 or Wayland, either is fine; on a headless
machine the `mosna` command line still works in full.

```bash
# Linux / macOS
./install.sh                     # into ~/.local, with a desktop launcher
./install.sh --prefix /usr/local # for everyone
./install.sh --dry-run           # show what would happen
./install.sh --uninstall         # remove it again
```

```powershell
# Windows, in one command — installs Rust if the machine has none,
# fetches the sources, builds and installs.
irm https://raw.githubusercontent.com/OwenGriere/MOSNA_Enhanced/main/bootstrap.ps1 | iex
```

That command runs a script fetched from the network. Reading it first is the
sensible thing to do with any such script, this one included:

```powershell
irm https://raw.githubusercontent.com/OwenGriere/MOSNA_Enhanced/main/bootstrap.ps1 -OutFile bootstrap.ps1
notepad bootstrap.ps1
.\bootstrap.ps1
```

From a clone you already have, skip the bootstrap and install directly:

```powershell
.\install.ps1                    # into %LOCALAPPDATA%\Programs\MOSNA
.\install.ps1 -DryRun
.\install.ps1 -Uninstall
```

Both create a desktop icon, as the Python `setup.sh` did, plus an application
menu entry (Start Menu on Windows). The full instructions are also in the
manual, inside the interface: **Viewer → Documentation → Installation**.

## Use

```bash
mosna-gui                        # the interface
mosna --help                     # the command line, for scripts and clusters
```

The command line runs the same four steps the interface's buttons do:

```bash
mosna tysserand-network --file CONFIG/configuration.yaml --working_dir /data/run
mosna assortativity     --file CONFIG/configuration.yaml --working_dir /data/run
mosna niche-analysis    --file CONFIG/configuration.yaml --working_dir /data/run
mosna clear-temporary   --file CONFIG/configuration.yaml --working_dir /data/run
```

## Design goals

1. **Identical usage.** Same `CONFIG/configuration.yaml`, same output file
   names and directory layout, same three analysis steps, same GUI layout, and
   the same `[QT_PROGRESS]` / `[QT_INFO]` protocol between the interface and the
   compute processes. Either interface can drive either backend.
2. **One file per function.** Every Python function has a Rust module of the
   same name, in a directory mirroring the Python package layout.
3. **No hidden numerical drift.** Where an algorithm had to be reimplemented,
   the port is tested against the mathematical definition, not against a
   transcription of the Python.
4. **Test-driven.** Every module was written after the tests that pin it. See
   `TESTING.md`.

## Layout

```
.
├── install.sh / install.ps1  build and install, per platform
├── CONFIG/                   the shipped starting configuration
├── assets/                   logo and the manual's figures
├── test/                     small real datasets the tests run against
├── benchmark/                the bench: drift, reproducibility, recovery, timings
├── PROGRESS.log              session journal and resumption point
├── TASKS.md                  what is left to do
├── TESTING.md                the testing discipline
├── Cargo.toml                workspace
└── crates/
    ├── mosna-config/       configuration.yaml model, round-trip, validation
    ├── mosna-io/           parquet/csv/tsv tables, network file discovery
    ├── mosna-core/         scientific core
    │   ├── geometry/       Delaunay, kNN, edge trimming  (<- tysserand)
    │   ├── nas/            Neighbors Aggregation Statistics
    │   ├── assortativity/  mixing matrices, permutation null, z-scores
    │   ├── linalg/         symmetric eigen, Cholesky, k-means
    │   ├── reduction/      UMAP
    │   ├── clustering/     GMM, Leiden, spectral
    │   ├── niches/         niche composition
    │   └── stats/          percentiles, Ward linkage, CLR
    ├── mosna-viz/          PNG figures
    ├── mosna-pipeline/     the four analyses
    ├── mosna-cli/          command line interface
    ├── mosna-gui/          graphical interface, and the bilingual manual
    ├── mosna-paths/        where things live, per platform
    └── mosna-install/      the installer, desktop and Start Menu shortcuts
```

## Python → Rust map

The Python sources this port replaces are no longer in the tree; the mapping is
kept because it names what each crate is responsible for.

| Python | Rust |
|---|---|
| `GUI_MOSNA.py` | `crates/mosna-gui` |
| `setup.sh` / `setup_windows.bat` | `install.sh` / `install.ps1`, `crates/mosna-install` |
| `assets/documentation.html` | `crates/mosna-gui/src/docs` |
| `package/tysserand_network.py` | `mosna-pipeline::tysserand_network` |
| `package/assortativity.py` | `mosna-pipeline::assortativity` |
| `package/niche_analysis.py` | `mosna-pipeline::niche_analysis` |
| `package/clear_temporary.py` | `mosna-pipeline::clear_temporary` |
| `package/utils/*` | `mosna-config`, `mosna-io` |
| `package/core/*` | `mosna-core`, `mosna-viz` |
| `mosna-package/mosna/neighbors.py` | `mosna-core::nas` |
| `mosna-package/mosna/assortativity.py` | `mosna-core::assortativity` |
| `mosna-package/mosna/clustering.py` | `mosna-core::{reduction, clustering}` |
| `mosna-package/mosna/niches.py` | `mosna-core::niches` |
| `mosna-package/mosna/plotting.py` | `mosna-viz` |
| `tysserand/tysserand.py` | `mosna-core::geometry` |

## Build and test

```bash
cargo build --release
cargo test --workspace
```

The release profile uses fat LTO and a single codegen unit. There is no LAPACK,
BLAS or Python dependency: the linear algebra needed (symmetric eigensolver,
Cholesky, k-means) is implemented in `mosna-core::linalg`, sized for the small
matrices that actually occur.

CI runs the same checks on Linux and Windows, with `-D warnings`, plus a deep
property-test pass — see `.github/workflows/rust.yml`.

```bash
cargo run --release -p mosna-bench -- all
```

runs the bench: numerical drift against recorded references, reproducibility
across thread counts, recovery of planted niches, and a timing sweep. See
`benchmark/README.md`.

## Status

See `PROGRESS.log`. It records what is implemented, every deliberate deviation
from the Python behaviour and why, and a precise resumption point.
