<#
.SYNOPSIS
    Install MOSNA for the current user.

.DESCRIPTION
    The Windows counterpart of install.sh. As there, the real work is done by
    the `mosna-install` binary, which is tested; this script only finds the
    binaries and hands over, so there is no untested logic in PowerShell.

    It runs in two situations and tells them apart on its own:

      * next to already-built binaries, as in a downloaded release — nothing is
        compiled, and neither Rust nor the Microsoft C++ build tools are needed;
      * in a source checkout — the binaries are built first, which does need
        both.

.EXAMPLE
    .\install.ps1
    Install into %LOCALAPPDATA%\Programs\MOSNA and create the shortcuts.

.EXAMPLE
    .\install.ps1 -NoFigures
    The analyses without the figures: skips the Python environment, which is
    the largest part of the install and the only part that needs Python at all.

.EXAMPLE
    .\install.ps1 -Prefix 'C:\Program Files\MOSNA'
    Install elsewhere. Writing under Program Files needs an elevated shell.

.EXAMPLE
    .\install.ps1 -DryRun
    Show what would happen, without touching the disk.

.EXAMPLE
    .\install.ps1 -Uninstall
    Remove a previous install, shortcuts included.
#>

[CmdletBinding()]
param(
    [string] $Prefix,
    [switch] $NoFigures,
    [switch] $DryRun,
    [switch] $Uninstall
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here

# ---------------------------------------------------------------------------
# Where the binaries are, and whether they have to be built
# ---------------------------------------------------------------------------

# A downloaded release carries the binaries beside this script; a checkout does
# not. Which one this is decides everything below, so it is settled first.
$packaged = Test-Path (Join-Path $here 'mosna-install.exe')
$buildDir = if ($packaged) { $here } else { Join-Path $here 'target\release' }
$installer = Join-Path $buildDir 'mosna-install.exe'

if (-not $packaged -and -not $Uninstall) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error @'
cargo was not found, and there are no pre-built binaries beside this script.

The quickest way in is not to build at all — the release page carries binaries
for Windows:

    https://github.com/OwenGriere/MOSNA_Enhanced/releases

To build from source instead, install the Rust toolchain from https://rustup.rs,
then open a new PowerShell window and run this script again.
'@
        exit 1
    }

    # Rust links Windows binaries with Microsoft's linker and does not ship one,
    # and one dependency (`zstd-sys`) is C, so the C compiler is needed too.
    # Both come with the same package. Checked here rather than discovered in a
    # linker error three minutes into a build.
    $hasLinker = [bool] (Get-Command link.exe -ErrorAction SilentlyContinue)
    if (-not $hasLinker) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (Test-Path $vswhere) {
            $found = & $vswhere -latest -products * `
                -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
                -property installationPath 2>$null
            $hasLinker = [bool] $found
        }
    }
    if (-not $hasLinker) {
        Write-Error @'
The Microsoft C++ build tools were not found, and building MOSNA needs them.

This is not specific to MOSNA: Rust links Windows programs with Microsoft's
linker (link.exe) and does not ship one of its own.

The quickest way out is not to build at all. The release page carries binaries
that need neither Rust nor Visual Studio:

    https://github.com/OwenGriere/MOSNA_Enhanced/releases

To build anyway, install the tools (several gigabytes), then open a NEW
PowerShell window and run this script again:

    winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override `
      "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
'@
        exit 1
    }
}

# ---------------------------------------------------------------------------
# Python, for the figures only
# ---------------------------------------------------------------------------

# The figures are drawn by the Python package `xy`, which the installer puts
# into an environment of its own under the prefix. Which versions are
# acceptable is the installer's rule, tested there; all this does is say so
# before a build rather than after one.
$renderer = Join-Path $here 'python'
if ($NoFigures -or -not (Test-Path $renderer)) {
    $renderer = $null
} elseif (-not $Uninstall) {
    $python = Get-Command python -ErrorAction SilentlyContinue
    if (-not $python) { $python = Get-Command python3 -ErrorAction SilentlyContinue }
    if (-not $python) {
        Write-Error @'
Python was not found.

MOSNA draws its figures with the Python package `xy`, so it needs Python 3.11
or newer. Install it from https://www.python.org/downloads/ or with:

    winget install Python.Python.3.13

then open a new PowerShell window and run this script again.

To install without the figures — the analyses still run and still write their
tables — pass -NoFigures instead.
'@
        exit 1
    }
}

# ---------------------------------------------------------------------------
# Build, if this is a checkout
# ---------------------------------------------------------------------------

if (-not $packaged -and -not $Uninstall) {
    Write-Host 'Building MOSNA (this takes a few minutes the first time)...'
    cargo build --release --bin mosna --bin mosna-gui --bin mosna-install
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

# ---------------------------------------------------------------------------
# Hand over to the installer
# ---------------------------------------------------------------------------

$arguments = @(
    '--build-dir', $buildDir,
    '--config',    (Join-Path $here 'CONFIG\configuration.yaml'),
    '--icon',      (Join-Path $here 'assets\logo.ico')
)
if ($renderer)  { $arguments += @('--renderer', $renderer) }
if ($Prefix)    { $arguments += @('--prefix', $Prefix) }
if ($DryRun)    { $arguments += '--dry-run' }
if ($Uninstall) { $arguments += '--uninstall' }

if ($packaged) {
    if (-not (Test-Path $installer)) {
        Write-Error "mosna-install.exe was not found in $buildDir — the download looks incomplete."
        exit 1
    }
    & $installer @arguments
} else {
    cargo run --release --quiet --bin mosna-install -- @arguments
}
exit $LASTEXITCODE
