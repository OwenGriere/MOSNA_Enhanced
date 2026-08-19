[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org/)

<h1 align="center">Mosna Enhanced : A version of Mosna rewritted in Rust for a better performance</h1>

Spatial network construction and analysis for spatial omics: a native
application that reconstructs a spatial network per sample, measures which cell
types sit next to which, and groups neighbourhoods into spatial niches.

This is the Rust implementation, and the root of the project. It replaces the
earlier Python/PySide6 version, keeping the same configuration file, the same
output layout and the same three-step workflow — an existing
`CONFIG/configuration.yaml` works unchanged.

## Install

The analyses are Rust and depend on no scientific stack. The **figures** are
drawn by [`xy`](https://github.com/reflex-dev/xy), a Python charting library,
which is why an interpreter is needed. What this needs, to build:

* the **Rust toolchain** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
* **Python 3.11 or newer**, and a **C compiler**:

  | | |
  |---|---|
  | Debian, Ubuntu, Mint | `sudo apt install build-essential python3 python3-venv` |
  | Fedora, RHEL | `sudo dnf install gcc python3` |
  | Arch, Manjaro | `sudo pacman -S base-devel python` |
  | openSUSE | `sudo zypper install gcc python3` |

`install.sh` checks for all of this before it starts building, and names the
command to run if something is missing. It then creates a virtual environment
of its own under the install prefix — `share/mosna/venv` — and installs the
renderer into it, so nothing is written into the Python you work in. The
graphical interface additionally needs a running desktop session — X11 or
Wayland, either is fine; on a headless machine the `mosna` command line still
works in full.

`MOSNA_PYTHON` overrides the interpreter, which is what to set when working
from a checkout against an environment of your own.

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

Every figure is written twice: a **PNG**, which is what the interface's gallery
shows, and an **HTML** chart beside it, which is the same figure with its axes
live — pan, zoom, and the value of a cell under the pointer.

**Generate report** gathers all of them into `report.html` at the root of the
working directory. One tab per analysis; inside each, the cohort's figures
first and then a patient at a time; a search box that filters by patient,
sample or file name; and thumbnails that open full size, where the chart zooms
and pans as it does on its own. A fourth tab lists everything else in the
directory. It references the figures rather than copying them, so a report of
five hundred figures is six hundred kilobytes and travels with the folder.

The command line runs the same steps the interface's buttons do:

```bash
mosna tysserand-network --file CONFIG/configuration.yaml --working_dir /data/run
mosna assortativity     --file CONFIG/configuration.yaml --working_dir /data/run
mosna niche-analysis    --file CONFIG/configuration.yaml --working_dir /data/run
mosna generate-report                                    --working_dir /data/run
mosna clear-temporary                                    --working_dir /data/run
```

The last two take no configuration: they act on a directory that already
exists, which is what lets them be run on results copied off a cluster.

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
├── python/                   the figure renderer, built on `xy`
├── test/                     small real datasets, and the testing discipline
├── benchmark/                the bench: drift, reproducibility, recovery, timings
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
    ├── mosna-xy/           figure specifications, handed to the renderer
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
| the manual | `crates/mosna-gui/src/docs` |
| `package/tysserand_network.py` | `mosna-pipeline::tysserand_network` |
| `package/assortativity.py` | `mosna-pipeline::assortativity` |
| `package/niche_analysis.py` | `mosna-pipeline::niche_analysis` |
| `package/clear_temporary.py` | `mosna-pipeline::clear_temporary` |
| — (new) | `mosna-pipeline::report` |
| `package/utils/*` | `mosna-config`, `mosna-io` |
| `package/core/*` | `mosna-core`, `mosna-xy`, `python/mosna_xy` |
| `mosna-package/mosna/neighbors.py` | `mosna-core::nas` |
| `mosna-package/mosna/assortativity.py` | `mosna-core::assortativity` |
| `mosna-package/mosna/clustering.py` | `mosna-core::{reduction, clustering}` |
| `mosna-package/mosna/niches.py` | `mosna-core::niches` |
| `mosna-package/mosna/plotting.py` | `mosna-xy`, `python/mosna_xy` |
| `tysserand/tysserand.py` | `mosna-core::geometry` |

## Build and test

```bash
cargo build --release
python3 -m venv .venv && .venv/bin/pip install -e python   # the figure renderer
cargo test --workspace
.venv/bin/python -m pytest python                          # and its own tests
```

The release profile uses fat LTO and a single codegen unit. There is no LAPACK
or BLAS dependency: the linear algebra needed (symmetric eigensolver, Cholesky,
k-means) is implemented in `mosna-core::linalg`, sized for the small matrices
that actually occur. Python appears in one place and one only — drawing the
figures — and the tests that check a figure was drawn run the real renderer.
A checkout with a `.venv` at its root is found automatically, which is what
lets `cargo test` draw figures from any crate's directory.

CI runs the same checks on Linux and Windows, with `-D warnings`, plus a deep
property-test pass — see `.github/workflows/rust.yml`.

```bash
cargo run --release -p mosna-bench -- all
```

runs the bench: numerical drift against recorded references, reproducibility
across thread counts, recovery of planted niches, and a timing sweep. See
`benchmark/README.md`.

## Status

The port is complete: the three analyses run, write their files and their
figures, the interface drives them, and the repository is self-contained.
`cargo test --workspace` and `pytest python` are the record of what is
guaranteed — every module ships with the tests that pin it, and
`test/TESTING.md` states the discipline they were written under.
