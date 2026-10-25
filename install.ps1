# Install the newest `ve` release on Windows.
#
#   irm https://raw.githubusercontent.com/binderg/snapcut-core/main/install.ps1 | iex
#
# $env:VE_INSTALL_DIR picks the folder (default %LOCALAPPDATA%\Programs\ve).
# $env:VE_VERSION installs a given tag (v0.1.0-rc.1) instead of the newest
# release. FFmpeg is not included: winget install Gyan.FFmpeg
$ErrorActionPreference = 'Stop'

$repo = 'binderg/snapcut-core'
$target = 'x86_64-pc-windows-msvc'
$dir = if ($env:VE_INSTALL_DIR) { $env:VE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\ve' }
$url = if ($env:VE_VERSION) {
    "https://github.com/$repo/releases/download/$($env:VE_VERSION)/ve-$target.zip"
} else {
    "https://github.com/$repo/releases/latest/download/ve-$target.zip"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("ve-install-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "downloading $url"
    Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmp 've.zip') -UseBasicParsing
    Expand-Archive -Path (Join-Path $tmp 've.zip') -DestinationPath $tmp
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Copy-Item (Join-Path $tmp "ve-$target\ve.exe") (Join-Path $dir 've.exe') -Force
    Write-Host "installed $dir\ve.exe"
} finally {
    Remove-Item -Recurse -Force $tmp
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not ($userPath -split ';' | Where-Object { $_ -eq $dir })) {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')
    $env:Path = "$env:Path;$dir"
    Write-Host "added $dir to your PATH (open a new terminal to use it)"
}
if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
    Write-Host 've needs FFmpeg: winget install Gyan.FFmpeg'
}
& (Join-Path $dir 've.exe') version
