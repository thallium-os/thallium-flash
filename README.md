# thallium-flash

> Downloads the Thallium 81 ISO from several mirrors at once and writes it straight to a USB drive. For anyone installing Thallium.

## Quick Start
Invite-only: you need an invited email. One command signs you in, downloads the ISO and flashes it.

Linux (opens a page: sign in, pick the USB drive, press Flash):
```sh
curl -fsSL https://raw.githubusercontent.com/thallium-os/thallium-flash/main/get.sh | sh
```
macOS (Apple Silicon) uses the same command and saves a verified ISO to Downloads. Windows (PowerShell):
```powershell
irm https://raw.githubusercontent.com/thallium-os/thallium-flash/main/get.ps1 | iex
```
Nothing is installed system-wide; files go in `~/.local/share/thallium-flash` (`%LOCALAPPDATA%\thallium-flash` on Windows).

## What it does
- Splits the ISO into hashed pieces (256 MiB each by default), so each file fits under GitHub's 2 GiB limit.
- Downloads 8 pieces in parallel from every listed mirror: R2 (via HTTP range requests on the full ISO) and GitHub Releases (part files).
- If a mirror fails or sends bad data, retries that piece on another mirror.
- Writes each piece to the USB drive as it arrives, without saving the ISO first, then reads the drive back and checks the hash.
- `--resume` keeps pieces that are already on the target and hash correctly.

## Usage
Easiest: `thallium-flash web` opens a local page in your browser (like Ventoy's WebUI). Pick the USB drive and press Flash. It asks for your password once (pkexec), only for the disk write. The page is served on 127.0.0.1 only and needs the per-run token in the printed link.

Release side:
```sh
thallium-flash split thallium-81-amd64.iso -o dist --version 0.6.12 \
  -m parts:https://github.com/thallium-os/thallium-flash/releases/download/iso-0.6.12
# attach dist/* to that GitHub release (mark it latest)
```
User side:
```sh
sudo thallium-flash flash /dev/sdX          # latest Thallium ISO to a USB disk
thallium-flash flash thallium.iso            # or to a file
thallium-flash flash -m ./manifest.json out.iso   # custom manifest
```

## Unattended install
The web page's **Set up the install in advance** section, or `--setup thallium-setup.toml`, adds a `THALLIUMCFG` partition with installer answers: account, hostname, time zone, Wi-Fi, channel and apps. Secrets are stored hashed. See [docs/setup-config.md](docs/setup-config.md).

## Limits
Writing to a USB drive is Linux-only for now. On Windows and macOS it can only write to a file. 

## License
TBD
