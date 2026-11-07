# Thallium Flash for Windows: sign in and download a verified Thallium 81 ISO.
#   irm https://raw.githubusercontent.com/thallium-os/thallium-flash/main/get.ps1 | iex
# Installs nothing system-wide: files go in %LOCALAPPDATA%\thallium-flash.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is very slow with the progress bar

$dir = Join-Path $env:LOCALAPPDATA 'thallium-flash'
New-Item -ItemType Directory -Force -Path $dir | Out-Null

Write-Host 'Getting Thallium Flash...'
$tf = Join-Path $dir 'thallium-flash.exe'
Invoke-WebRequest 'https://github.com/thallium-os/thallium-flash/releases/latest/download/thallium-flash-windows-x86_64.exe' -OutFile $tf

$cf = Join-Path $dir 'cloudflared.exe'
if (-not (Get-Command cloudflared -ErrorAction SilentlyContinue) -and -not (Test-Path $cf)) {
    Write-Host 'Getting cloudflared (handles the sign-in)...'
    Invoke-WebRequest 'https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe' -OutFile $cf
}
$env:PATH = "$dir;$env:PATH"

$iso = Join-Path $HOME 'Downloads\thallium-81-amd64.iso'
& $tf flash $iso
if ($LASTEXITCODE -ne 0) { throw 'Download failed (see the message above).' }
Write-Host ''
Write-Host "Saved and verified: $iso"
Write-Host 'Write it to a USB drive with Rufus (https://rufus.ie) in DD mode.'
