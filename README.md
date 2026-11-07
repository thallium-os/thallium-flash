# thallium-flash

> Downloads the Thallium 81 ISO from several mirrors at once and writes it straight to a USB drive. For anyone installing Thallium.

## What it does
- Splits the ISO into hashed pieces (256 MiB each by default), so each file fits under GitHub's 2 GiB limit.
- Downloads 8 pieces in parallel from every listed mirror: R2 (via HTTP range requests on the full ISO) and GitHub Releases (part files).
- If a mirror fails or sends bad data, retries that piece on another mirror.
- Writes each piece to the USB drive as it arrives, without saving the ISO first, then reads the drive back and checks the hash.
- `--resume` keeps pieces that are already on the target and hash correctly.

## Usage
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

## Limits
Writing to a USB drive is Linux-only for now. On Windows and macOS it can only write to a file. A GUI comes later.

## License
TBD
