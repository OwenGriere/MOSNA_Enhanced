<#
.SYNOPSIS
    Build MOSNA and install it for the current user.

.DESCRIPTION
    The Windows counterpart of install.sh. As there, the real work is done by
    the `mosna-install` binary, which is tested; this script only builds and
    hands over, so there is no untested logic in PowerShell.

.EXAMPLE
    .\install.ps1
    Install into %LOCALAPPDATA%\Programs\MOSNA and create the shortcuts.

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
    [switch] $DryRun,
    [switch] $Uninstall
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error @'
cargo was not found.

MOSNA is built with Rust. Install the toolchain from:

    https://rustup.rs

then open a new PowerShell window and run this script again.
'@
    exit 1
}

# -Uninstall needs no build, and refusing to run without one would leave a user
# unable to clean up after a failed install.
if (-not $Uninstall) {
    # The figures are drawn by the Python package `xy`, which the installer
    # puts into an environment of its own under the prefix. Which versions are
    # acceptable is the installer's rule, tested there; all this does is say so
    # before a build rather than after one.
    $python = Get-Command python -ErrorAction SilentlyContinue
    if (-not $python) { $python = Get-Command python3 -ErrorAction SilentlyContinue }
    if (-not $python) {
        Write-Error @'
Python was not found.

MOSNA draws its figures with the Python package `xy`, so it needs Python 3.11
or newer. Install it from https://www.python.org/downloads/ or with:

    winget install Python.Python.3.13

then open a new PowerShell window and run this script again.
'@
        exit 1
    }

    Write-Host 'Building MOSNA (this takes a few minutes the first time)...'
    cargo build --release --bin mosna --bin mosna-gui
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$arguments = @(
    '--build-dir', (Join-Path $here 'target\release'),
    '--config',    (Join-Path $here 'CONFIG\configuration.yaml'),
    '--icon',      (Join-Path $here 'assets\logo.ico'),
    '--renderer',  (Join-Path $here 'python')
)
if ($Prefix)   { $arguments += @('--prefix', $Prefix) }
if ($DryRun)   { $arguments += '--dry-run' }
if ($Uninstall) { $arguments += '--uninstall' }

cargo run --release --quiet --bin mosna-install -- @arguments
exit $LASTEXITCODE
