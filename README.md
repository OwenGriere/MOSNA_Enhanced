<p align="center">
  <a href="https://www.rust-lang.org/"><img alt="Rust 1.82 or newer" src="https://img.shields.io/badge/Rust-1.82%2B-CE422B?style=flat-square&logo=rust&logoColor=white"></a>
  <a href="https://www.python.org/downloads/"><img alt="Python 3.11 or newer" src="https://img.shields.io/badge/Python-3.11%2B-3776AB?style=flat-square&logo=python&logoColor=white"></a>
  <a href="https://opensource.org/licenses/MIT"><img alt="MIT licence" src="https://img.shields.io/badge/Licence-MIT-75590C?style=flat-square"></a>
</p>

<p align="center">
  <sub><b>Figures</b> — drawn by a Python renderer, pinned exactly</sub><br>
  <a href="https://github.com/reflex-dev/xy"><img alt="xy 0.0.6" src="https://img.shields.io/badge/xy-0.0.6-4B8BBE?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://numpy.org/"><img alt="numpy 1.24 or newer" src="https://img.shields.io/badge/numpy-%E2%89%A51.24-4DABCF?style=flat-square&labelColor=2B2F36&logo=numpy&logoColor=white"></a>
</p>

<p align="center">
  <sub><b>Analyses, I/O and interface</b> — from crates.io, at the versions <code>Cargo.lock</code> resolves</sub><br>
  <a href="https://crates.io/crates/egui"><img alt="egui 0.35" src="https://img.shields.io/badge/egui-0.35-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/eframe"><img alt="eframe 0.35" src="https://img.shields.io/badge/eframe-0.35-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/arrow-array"><img alt="arrow-array 56.2" src="https://img.shields.io/badge/arrow--array-56.2-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/parquet"><img alt="parquet 56.2" src="https://img.shields.io/badge/parquet-56.2-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/rayon"><img alt="rayon 1.12" src="https://img.shields.io/badge/rayon-1.12-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/ndarray"><img alt="ndarray 0.16" src="https://img.shields.io/badge/ndarray-0.16-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/kiddo"><img alt="kiddo 5.3" src="https://img.shields.io/badge/kiddo-5.3-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/delaunator"><img alt="delaunator 1.1" src="https://img.shields.io/badge/delaunator-1.1-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/clap"><img alt="clap 4.6" src="https://img.shields.io/badge/clap-4.6-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/serde"><img alt="serde 1.0" src="https://img.shields.io/badge/serde-1.0-6D747C?style=flat-square&labelColor=2B2F36"></a>
  <a href="https://crates.io/crates/proptest"><img alt="proptest 1.11" src="https://img.shields.io/badge/proptest-1.11-6D747C?style=flat-square&labelColor=2B2F36"></a>
</p>

<h1 align="center">Mosna Enhanced : A version of Mosna rewritted in Rust for a better performance</h1>

Spatial network construction and analysis for spatial omics: a native
application that reconstructs a spatial network per sample, measures which cell
types sit next to which, and groups neighbourhoods into spatial niches.

This is the Rust implementation, and the root of the project. It replaces the
earlier Python/PySide6 version, keeping the same configuration file, the same
output layout and the same three-step workflow — an existing
`CONFIG/configuration.yaml` works unchanged.

## Install

| | Windows | Linux (and macOS) |
|---|---|---|
| installer | `INSTALLATION.exe`, double-clicked | `./install.sh`, in a terminal |
| prerequisites | none, it installs what is missing | Rust 1.82+, a C compiler, Python 3.11+ |
| installed into | `%LOCALAPPDATA%\Programs\MOSNA` | `~/.local` (or `--prefix`) |
| first install | 20 to 40 minutes | a few minutes |

Both build MOSNA from the sources, then hand over to the same installer,
`mosna-install`, so the result is the same on both: the `mosna` and
`mosna-gui` binaries, the starting configuration, the icon, a desktop shortcut
and an application menu entry, and — unless the figures are left out — a
Python environment of its own for the figure renderer. Nothing is written into
the Python you work in.

Each installer then deletes the other's: once installed, `INSTALLATION.exe`
removes `install.sh` and `install.sh` removes `INSTALLATION.exe` and
`UNINSTALL.exe`, so an
installed folder only keeps the files for its own system.

### Windows — `INSTALLATION.exe`

1. Download the repository: **Code → Download ZIP**, then right-click the ZIP
   → **Extract all**. `INSTALLATION.exe` does not work from inside the ZIP: it
   has to sit in the extracted folder, next to `Cargo.toml`.
2. Open the extracted folder and double-click **`INSTALLATION.exe`**.
3. A window asks for:
   * **the folder to put MOSNA in** — by default `MOSNA_Enhanced` in your
     user folder (`C:\Users\<you>\MOSNA_Enhanced`), since `Downloads` is no
     place to keep it. A folder picked with **Parcourir…** receives a
     `MOSNA_Enhanced` folder inside it; it must be empty, new, or an earlier
     copy of MOSNA;
   * **a desktop shortcut** — ticked by default; the Start Menu entry is
     written either way;
   * **the figures module** (Python, about 85 MB) — ticked by default.
4. Click **Installer**. The window then:
   1. moves the folder to the chosen place (the downloaded copy is removed);
   2. installs what is missing, each only if it is not already there: the
      **Microsoft C++ build tools** (several GB, the longest step — Windows
      asks once for administrator rights), **Rust**, and **Python 3.12** when
      the figures are wanted. It uses `winget` and falls back to the official
      installers;
   3. compiles MOSNA (10 to 20 minutes the first time);
   4. installs it into `%LOCALAPPDATA%\Programs\MOSNA`, with the shortcuts;
   5. deletes `install.sh`.
5. At the end, **Lancer MOSNA** opens the interface. Afterwards, start it from
   the desktop shortcut or the Start Menu.

Leave the window open until it says so: it cannot be closed while installing.
Everything it prints is also kept in `%TEMP%\mosna-install.log`, which is the
file to look at, or to send, when something fails.

**To update**, download and extract the new version and run its
`INSTALLATION.exe`, choosing the folder MOSNA is already in. The window
recognises the earlier copy and offers **Mettre à jour et installer**: your
results and your configuration are kept, only MOSNA's own files are replaced.
What is already installed (build tools, Rust, Python) is not reinstalled, and
the build reuses the previous one, so an update is much faster.

**To uninstall**, double-click **`UNINSTALL.exe`** in MOSNA's folder. It
always removes MOSNA, its shortcuts and its figures environment, and the build
(`target\`, several GB); your configuration is kept. It also offers to remove:

* **the C++ build tools, Rust and Python 3.12**, each one found on the machine.
  Those `INSTALLATION.exe` installed itself are ticked: it writes them down in
  `.mosna-prerequisites`, at the root of the folder. The others are left
  unticked, since other software may use them. Removing the build tools asks
  for administrator rights, and leaves the Visual Studio Installer itself, to
  be removed from Settings → Apps if nothing else needs it.
* **the MOSNA folder itself**, unticked: it holds whatever you put there,
  results included.

Its log is kept in `%TEMP%\mosna-uninstall.log`. To remove MOSNA alone from a
terminal instead:

```bat
target\release\mosna-install.exe --uninstall
```

The command line is installed but not added to the `PATH`; from a terminal, use
`%LOCALAPPDATA%\Programs\MOSNA\bin\mosna.exe`.

If the window does not open at all, the message that appears is almost always
about the graphics driver (no OpenGL 2): update it, then run
`INSTALLATION.exe` again.

### Linux — `install.sh`

What it needs, to build:

* the **Rust toolchain**, 1.82 or newer — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
* a **C compiler**, and **Python 3.11 or newer** for the figures:

  | | |
  |---|---|
  | Debian, Ubuntu, Mint | `sudo apt install build-essential python3 python3-venv` |
  | Fedora, RHEL | `sudo dnf install gcc python3` |
  | Arch, Manjaro | `sudo pacman -S base-devel python` |
  | openSUSE | `sudo zypper install gcc python3` |

`install.sh` checks for all of this before it starts building, and names the
command to run if something is missing — Python is only asked for when the
figures are wanted.

```bash
git clone https://github.com/OwenGriere/MOSNA_Enhanced.git
cd MOSNA_Enhanced
./install.sh                     # into ~/.local, with a desktop launcher
```

| option | |
|---|---|
| `--no-figures` | smallest install: no Python environment |
| `--prefix /usr/local` | somewhere else, here for everyone (needs write access) |
| `--no-desktop-shortcut` | no launcher on the desktop; the menu entry is still written |
| `--dry-run` | show what would be done, without touching the disk |
| `--uninstall` | remove it again — with the same `--prefix`, if one was given |

It builds `mosna`, `mosna-gui` and `mosna-install` with `cargo build
--release`, then installs:

| | |
|---|---|
| `~/.local/bin/mosna`, `~/.local/bin/mosna-gui` | the command line and the interface |
| `~/.local/share/mosna/` | the starting configuration, and the figures' environment, `venv/` |
| `~/.local/share/applications/`, the desktop | the launchers, with the icon |

and, once the install has succeeded, deletes `INSTALLATION.exe` and
`UNINSTALL.exe` (not on
`--dry-run`, `--uninstall` or `--help`). If `~/.local/bin` is not on your
`PATH`, which some distributions do not do, the installer says so and gives
the full path to the interface.

Run next to already-built binaries — the `mosna-linux-x86_64.tar.gz` of a
[release](https://github.com/OwenGriere/MOSNA_Enhanced/releases), unpacked —
the same script compiles nothing and needs no Rust nor C compiler.

The graphical interface needs a running desktop session — X11 or Wayland,
either is fine; on a headless machine the `mosna` command line still works in
full. `MOSNA_PYTHON` overrides the interpreter used for the figures, which is
what to set when working from a checkout against an environment of your own.

### How much it weighs

| | size | needed for |
|---|---:|---|
| `mosna` and `mosna-gui` | 38 MB | everything |
| Python environment (`xy`, numpy) | ~85 MB | the figures only |

The analyses are Rust and depend on no scientific stack; the **figures** are
drawn by [`xy`](https://github.com/reflex-dev/xy), a Python charting library,
which is the only reason Python is needed. `--no-figures` (or unticking the
figures module on Windows) skips the second row: the analyses then run exactly
as before and write all of their tables; only the images are left undrawn, and
each run says so instead of failing. Installing the renderer later draws the
figures the next run produces.

The full instructions are also in the manual, inside the interface:
**Viewer → Documentation → Installation**.

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
├── install.sh / INSTALLATION.exe  build and install, per platform
├── UNINSTALL.exe             remove it again, on Windows
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
| `setup.sh` / `setup_windows.bat` | `install.sh` / `INSTALLATION.exe`, `crates/mosna-install`, `crates/mosna-setup` |
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
