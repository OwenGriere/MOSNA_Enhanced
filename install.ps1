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
    Write-Host 'Building MOSNA (this takes a few minutes the first time)...'
    cargo build --release --bin mosna --bin mosna-gui
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$arguments = @(
    '--build-dir', (Join-Path $here 'target\release'),
    '--config',    (Join-Path $here 'CONFIG\configuration.yaml'),
    '--icon',      (Join-Path $here 'assets\logo.ico')
)
if ($Prefix)   { $arguments += @('--prefix', $Prefix) }
if ($DryRun)   { $arguments += '--dry-run' }
if ($Uninstall) { $arguments += '--uninstall' }

cargo run --release --quiet --bin mosna-install -- @arguments
exit $LASTEXITCODE
