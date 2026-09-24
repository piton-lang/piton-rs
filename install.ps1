# Installs piton on Windows.
#
#   irm https://github.com/piton-lang/piton-rs/releases/latest/download/install.ps1 | iex
#
# Settings, all optional:
#   $env:PITON_VERSION      a version to install, like 0.1.41 (default: the latest)
#   $env:PITON_INSTALL_DIR  where to put piton (default: %LOCALAPPDATA%\piton\bin)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Repo = 'piton-lang/piton-rs'
$Base = if ($env:PITON_DOWNLOAD_BASE) { $env:PITON_DOWNLOAD_BASE } else { "https://github.com/$Repo/releases" }
$InstallDir = if ($env:PITON_INSTALL_DIR) { $env:PITON_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'piton\bin' }

function Say($Message) { Write-Host "piton: $Message" }
function Fail($Message) { Write-Error "piton: $Message"; exit 1 }

# There is only an x86_64 build for Windows so far. Windows on ARM runs it
# through emulation.
$Arch = 'x86_64'

$Version = $env:PITON_VERSION
if ($Version) {
    $Version = $Version.TrimStart('v')
} else {
    # GitHub redirects /releases/latest to /releases/tag/vX.Y.Z. Following the
    # redirect avoids the API and its rate limit.
    $Response = Invoke-WebRequest -Uri "$Base/latest" -UseBasicParsing
    $Final = $Response.BaseResponse.ResponseUri
    if (-not $Final) { $Final = $Response.BaseResponse.RequestMessage.RequestUri }
    $Version = ($Final.AbsoluteUri -split '/v')[-1]
    if ($Version -notmatch '^\d') { Fail "couldn't find the latest release at $Base/latest" }
}

$Archive = "piton-edge-windows-$Arch-$Version.zip"
$Url = "$Base/download/v$Version/$Archive"

$Work = Join-Path ([System.IO.Path]::GetTempPath()) ("piton-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $Work | Out-Null

try {
    Say "downloading piton $Version for windows $Arch"
    $ArchivePath = Join-Path $Work $Archive
    Invoke-WebRequest -Uri $Url -OutFile $ArchivePath -UseBasicParsing
    Invoke-WebRequest -Uri "$Url.sha256" -OutFile "$ArchivePath.sha256" -UseBasicParsing

    $Expected = ((Get-Content "$ArchivePath.sha256" -Raw).Trim() -split '\s+')[0].ToLower()
    $Actual = (Get-FileHash -Algorithm SHA256 $ArchivePath).Hash.ToLower()
    if ($Expected -ne $Actual) { Fail "the download doesn't match its checksum; try again" }

    $Unpacked = Join-Path $Work 'unpacked'
    Expand-Archive -Path $ArchivePath -DestinationPath $Unpacked

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $Target = Join-Path $InstallDir 'piton.exe'
    Copy-Item (Join-Path $Unpacked 'piton.exe') $Target -Force

    $Installed = & $Target --version
    Say "installed $Installed to $Target"

    # Add the install directory to the user's PATH, once.
    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $Entries = if ($UserPath) { $UserPath -split ';' } else { @() }
    if ($Entries -notcontains $InstallDir) {
        $NewPath = (@($Entries | Where-Object { $_ }) + $InstallDir) -join ';'
        [Environment]::SetEnvironmentVariable('Path', $NewPath, 'User')
        Say "added $InstallDir to your PATH; open a new terminal to use piton"
    }
} finally {
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
