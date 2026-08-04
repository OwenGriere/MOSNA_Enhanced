<#
.SYNOPSIS
    Install MOSNA on Windows in one command.

.DESCRIPTION
    Fetches the sources, installs the Rust toolchain if the machine has none,
    and hands over to install.ps1 — which is the tested installer. Nothing is
    installed by this script itself; it only gets the machine to the point
    where install.ps1 can run.

    The one-line form, which is how most people will run it:

        irm https://raw.githubusercontent.com/OwenGriere/MOSNA_Enhanced/main/bootstrap.ps1 | iex

    Piped into `iex` a script receives no arguments, so every parameter below
    has a default that works. To pass one, download the script first:

        irm https://raw.githubusercontent.com/OwenGriere/MOSNA_Enhanced/main/bootstrap.ps1 -OutFile bootstrap.ps1
        .\bootstrap.ps1 -Path 'D:\src\MOSNA'

    Reading it before running it is the sensible thing to do with any script
    fetched from the network, this one included.

.PARAMETER Path
    Where to put the sources. Defaults to MOSNA_Enhanced in your user folder.

.PARAMETER Force
    Reuse the directory even if it holds something that is not a MOSNA
    checkout. Off by default: the script will not overwrite what it did not
    put there.

.PARAMETER DryRun
    Fetch and build, then show what installing would do without doing it.
#>

[CmdletBinding()]
param(
    [string] $Path,
    [switch] $Force,
    [switch] $DryRun
)

$ErrorActionPreference = 'Stop'

$Repository = 'https://github.com/OwenGriere/MOSNA_Enhanced'
$Branch = 'main'

if (-not $Path) {
    $Path = Join-Path $env:USERPROFILE 'MOSNA_Enhanced'
}

function Write-Step($message) {
    Write-Host ""
    Write-Host "==> $message" -ForegroundColor Yellow
}

# ---------------------------------------------------------------------------
# 1. The Rust toolchain
# ---------------------------------------------------------------------------

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Step 'Rust is already installed.'
} else {
    Write-Step 'Installing the Rust toolchain (this is a one-off)...'

    $installer = Join-Path $env:TEMP 'rustup-init.exe'
    Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile $installer -UseBasicParsing

    # -y takes the defaults; without it rustup waits for a keypress, which a
    # piped session never sends.
    & $installer -y --default-toolchain stable --profile minimal
    if ($LASTEXITCODE -ne 0) {
        Write-Error 'The Rust installer failed. Install it by hand from https://rustup.rs and run this again.'
        exit 1
    }

    # rustup puts cargo on the PATH for *new* shells; this one has to be told.
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}

# ---------------------------------------------------------------------------
# 2. The sources
# ---------------------------------------------------------------------------

$hasGit = [bool] (Get-Command git -ErrorAction SilentlyContinue)

if (Test-Path (Join-Path $Path '.git')) {
    Write-Step "Updating the existing checkout in $Path..."
    if ($hasGit) {
        git -C $Path pull --ff-only
    } else {
        Write-Host 'git is not installed, so the checkout cannot be updated; using it as it is.'
    }
} elseif ((Test-Path $Path) -and (Get-ChildItem $Path -Force | Select-Object -First 1)) {
    # Something is already there and it is not a checkout. Refuse rather than
    # write into a directory this script did not create.
    if (-not $Force) {
        Write-Error @"
$Path already exists and is not a MOSNA checkout.

Choose somewhere else with -Path, or pass -Force to use it anyway.
"@
        exit 1
    }
    Write-Step "Using the existing $Path as instructed."
} elseif ($hasGit) {
    Write-Step "Cloning $Repository into $Path..."
    git clone --branch $Branch --depth 1 "$Repository.git" $Path
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    # No git: GitHub serves a zip of any branch, which is enough to install
    # from. Updating later will mean running this script again.
    Write-Step "Downloading $Repository ($Branch)..."

    $archive = Join-Path $env:TEMP "mosna-$Branch.zip"
    $unpacked = Join-Path $env:TEMP "mosna-$Branch-unpacked"

    Invoke-WebRequest -Uri "$Repository/archive/refs/heads/$Branch.zip" -OutFile $archive -UseBasicParsing
    if (Test-Path $unpacked) { Remove-Item $unpacked -Recurse -Force }
    Expand-Archive -Path $archive -DestinationPath $unpacked -Force

    # The zip holds a single top-level directory named after the branch.
    $inner = Get-ChildItem $unpacked -Directory | Select-Object -First 1
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
    Copy-Item (Join-Path $inner.FullName '*') $Path -Recurse -Force

    Remove-Item $archive, $unpacked -Recurse -Force
}

# ---------------------------------------------------------------------------
# 3. Hand over to the installer
# ---------------------------------------------------------------------------

$installer = Join-Path $Path 'install.ps1'
if (-not (Test-Path $installer)) {
    Write-Error "install.ps1 was not found in $Path — the download looks incomplete."
    exit 1
}

Write-Step 'Building and installing MOSNA (the first build takes a few minutes)...'
Set-Location $Path

if ($DryRun) {
    & $installer -DryRun
} else {
    & $installer
}
$code = $LASTEXITCODE

if ($code -eq 0) {
    Write-Host ""
    Write-Host 'MOSNA is installed. Start it from the desktop shortcut, from the' -ForegroundColor Green
    Write-Host 'Start Menu, or by running mosna-gui.' -ForegroundColor Green
    Write-Host ""
    Write-Host "Sources are in $Path. To remove MOSNA:  cd '$Path'; .\install.ps1 -Uninstall"
}
exit $code
