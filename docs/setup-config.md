# thallium-setup.toml: unattended install answers

Written by `thallium-flash flash --setup FILE` or by the GUI's Customize section. The installer reads it to skip questions.

## Where it lives
A 32 MiB FAT16 partition labelled `THALLIUMCFG`, added in MBR slot 3 or 4 right after the ISO, aligned to 1 MiB. The file is `/thallium-setup.toml` on that partition.
It sits outside the ISO's own data, so the ISO's sha256 still verifies. The MBR entry is added after the verify step.

Installer lookup: `/dev/disk/by-label/THALLIUMCFG`. If it's missing, run the normal interactive install.

## Format (version 1)
Every section is optional. A missing field means "ask the user".
```toml
version = 1

[account]
username = "neel"            # [a-z_][a-z0-9_-]{0,31}
full_name = "Neel"
password_hash = "$6$..."      # crypt(3) SHA-512 ($6$) or yescrypt ($y$); `useradd -p` takes it as-is
autologin = true

[system]
hostname = "thallium-laptop"
timezone = "Asia/Kolkata"     # IANA zone
locale = "en_IN.UTF-8"
keyboard = "us"               # XKB layout

[wifi]
ssid = "home"
psk = "<64 hex>"              # PBKDF2-HMAC-SHA1(pass, ssid, 4096, 32) = wpa_passphrase output; NetworkManager psk= accepts it

[thallium]
channel = "vanguard"          # vanguard | edgerunners
theme = "dark"                # dark | light
apps = ["thallium-music"]     # extra packages after the base install
skip_setup = true             # skip first-boot welcome screens

[disk]
mode = "ask"                  # ask | erase
target = "/dev/nvme0n1"       # required for erase
confirm_erase = false         # erase runs unattended ONLY if true
```

## Rules for the installer
- Reject plain-text secrets: `password_hash` must start with `$6$` or `$y$`, and `psk` must be 64 hex characters. `SetupConfig::validate` in `src/setup.rs` is the reference.
- Never erase a disk unless `mode = "erase"`, `target` is set, *and* `confirm_erase = true`. Otherwise keep the disk-selection screen. The GUI never writes a `[disk]` section.
- Unknown keys are an error (`deny_unknown_fields`), so new fields need `version = 2`.
- The partition stays readable on the USB drive. Don't copy the file onto the installed system.

## By hand
```sh
openssl passwd -6            # password_hash
wpa_passphrase "SSID" "pass" # psk= line
sudo thallium-flash flash --setup thallium-setup.toml /dev/sdX
```
