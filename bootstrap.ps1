<#
.SYNOPSIS
    Install MOSNA on Windows in one command.

.DESCRIPTION
    Downloads the binaries published for the latest release and installs them.
    Nothing is compiled, so neither the Rust toolchain nor the Microsoft C++
    build tools are needed — about five gigabytes of prerequisites that only
    ever existed to produce two files.

    The one-line form, which is how most people will run it:

        irm https://raw.githubusercontent.com/OwenGriere/MOSNA_Enhanced/main/bootstrap.ps1 | iex

    Piped into `iex` a script receives no arguments, so every parameter below
    has a default that works. To pass one, download the script first:

        irm https://raw.githubusercontent.com/OwenGriere/MOSNA_Enhanced/main/bootstrap.ps1 -OutFile bootstrap.ps1
        .\bootstrap.ps1 -NoFigures

    Reading it before running it is the sensible thing to do with any script
    fetched from the network, this one included.

.PARAMETER Path
    Where to put MOSNA's files. Defaults to MOSNA_Enhanced in your user folder.

.PARAMETER NoFigures
    Skip the Python environment used to draw figures. The analyses still run and
    still write their tables; this is the smallest install there is.

.PARAMETER FromSource
    Build from a checkout instead of downloading. Needs the Rust toolchain and
    the Microsoft C++ build tools; install.ps1 says so precisely if they are
    missing.

.PARAMETER Force
    Reuse the directory even if it holds something MOSNA did not put there.

.PARAMETER DryRun
    Show what installing would do without doing it.
#>

[CmdletBinding()]
param(
    [string] $Path,
    [switch] $NoFigures,
    [switch] $FromSource,
    [switch] $Force,
    [switch] $DryRun
)

$ErrorActionPreference = 'Stop'

$Repository = 'https://github.com/OwenGriere/MOSNA_Enhanced'
$Branch = 'main'
$AssetPattern = 'mosna-windows-*.zip'

if (-not $Path) {
    $Path = Join-Path $env:USERPROFILE 'MOSNA_Enhanced'
}

function Write-Step($message) {
    Write-Host ""
    Write-Host "==> $message" -ForegroundColor Yellow
}

# Refuse to write into a directory this script did not create, unless told to.
function Assert-Usable($directory) {
    if ((Test-Path $directory) -and
        (Get-ChildItem $directory -Force -ErrorAction SilentlyContinue | Select-Object -First 1)) {
        $ours = (Test-Path (Join-Path $directory 'mosna-install.exe')) -or
                (Test-Path (Join-Path $directory '.git'))
        if (-not $ours -and -not $Force) {
            Write-Error @"
$directory already exists and does not look like a MOSNA directory.

Choose somewhere else with -Path, or pass -Force to use it anyway.
"@
            exit 1
        }
    }
}

# ---------------------------------------------------------------------------
# The short way: the published binaries
# ---------------------------------------------------------------------------

function Install-FromRelease {
    Write-Step 'Looking for the latest release...'

    $api = 'https://api.github.com/repos/OwenGriere/MOSNA_Enhanced/releases/latest'
    try {
        # GitHub's API refuses requests without a user agent.
        $release = Invoke-RestMethod -Uri $api -Headers @{ 'User-Agent' = 'mosna-bootstrap' }
    } catch {
        Write-Host "The release list could not be read: $($_.Exception.Message)"
        return $false
    }

    $asset = $release.assets | Where-Object { $_.name -like $AssetPattern } | Select-Object -First 1
    if (-not $asset) {
        Write-Host "No Windows package in release $($release.tag_name)."
        return $false
    }

    Write-Step "Downloading MOSNA $($release.tag_name) ($([math]::Round($asset.size / 1MB, 1)) MB)..."

    $archive = Join-Path $env:TEMP $asset.name
    $unpacked = Join-Path $env:TEMP 'mosna-package'
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archive -UseBasicParsing

    if (Test-Path $unpacked) { Remove-Item $unpacked -Recurse -Force }
    Expand-Archive -Path $archive -DestinationPath $unpacked -Force

    # The archive holds a single top-level `mosna` directory.
    $inner = Join-Path $unpacked 'mosna'
    if (-not (Test-Path $inner)) {
        $inner = (Get-ChildItem $unpacked -Directory | Select-Object -First 1).FullName
    }

    Assert-Usable $Path
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
    Copy-Item (Join-Path $inner '*') $Path -Recurse -Force
    Remove-Item $archive, $unpacked -Recurse -Force

    return $true
}

# ---------------------------------------------------------------------------
# The long way: a checkout and a compiler
# ---------------------------------------------------------------------------

function Install-FromSource {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Step 'Rust is already installed.'
    } else {
        Write-Step 'Installing the Rust toolchain (this is a one-off)...'
        $rustup = Join-Path $env:TEMP 'rustup-init.exe'
        Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile $rustup -UseBasicParsing

        # -y takes the defaults; without it rustup waits for a keypress, which a
        # piped session never sends.
        & $rustup -y --default-toolchain stable --profile minimal
        if ($LASTEXITCODE -ne 0) {
            Write-Error 'The Rust installer failed. Install it by hand from https://rustup.rs and run this again.'
            exit 1
        }
        # rustup puts cargo on the PATH for *new* shells; this one has to be told.
        $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    }

    Assert-Usable $Path
    $hasGit = [bool] (Get-Command git -ErrorAction SilentlyContinue)

    if (Test-Path (Join-Path $Path '.git')) {
        Write-Step "Updating the existing checkout in $Path..."
        if ($hasGit) {
            git -C $Path pull --ff-only
        } else {
            Write-Host 'git is not installed, so the checkout cannot be updated; using it as it is.'
        }
    } elseif ($hasGit) {
        Write-Step "Cloning $Repository into $Path..."
        git clone --branch $Branch --depth 1 "$Repository.git" $Path
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } else {
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
}

# ---------------------------------------------------------------------------
# Get the files here, then hand over to install.ps1
# ---------------------------------------------------------------------------

$fromRelease = $false
if (-not $FromSource) {
    $fromRelease = Install-FromRelease
    if (-not $fromRelease) {
        Write-Host ''
        Write-Host 'Falling back to building from source, which needs the Rust toolchain' -ForegroundColor Yellow
        Write-Host 'and the Microsoft C++ build tools.' -ForegroundColor Yellow
    }
}
if (-not $fromRelease) {
    Install-FromSource
}

$installer = Join-Path $Path 'install.ps1'
if (-not (Test-Path $installer)) {
    Write-Error "install.ps1 was not found in $Path — the download looks incomplete."
    exit 1
}

if ($fromRelease) {
    Write-Step 'Installing MOSNA...'
} else {
    Write-Step 'Building and installing MOSNA (the first build takes a few minutes)...'
}
Set-Location $Path

$arguments = @()
if ($NoFigures) { $arguments += '-NoFigures' }
if ($DryRun)    { $arguments += '-DryRun' }
& $installer @arguments
$code = $LASTEXITCODE

if ($code -eq 0) {
    Write-Host ""
    Write-Host 'MOSNA is installed. Start it from the desktop shortcut, from the' -ForegroundColor Green
    Write-Host 'Start Menu, or by running mosna-gui.' -ForegroundColor Green
    Write-Host ""
    Write-Host "Files are in $Path. To remove MOSNA:  cd '$Path'; .\install.ps1 -Uninstall"
}
exit $code
