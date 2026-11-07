//! `thallium-setup.toml`: answers the installer reads instead of asking,
//! stored on a small FAT partition (label THALLIUMCFG) added after the ISO.
//! Every section is optional; anything missing is asked for as usual.
//! Secrets are stored only as hashes (SHA-512 crypt, WPA PSK).

use crate::target::{read_at, write_at};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;

pub const LABEL: &[u8; 11] = b"THALLIUMCFG";
pub const FILE_NAME: &str = "thallium-setup.toml";
const MIB: u64 = 1024 * 1024;
const PART_SIZE: u64 = 32 * MIB;
const SECTOR: u64 = 512;

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SetupConfig {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<Account>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<System>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wifi: Option<Wifi>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thallium: Option<Thallium>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<Disk>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// crypt(3) hash: `$6$…` (SHA-512) or `$y$…` (yescrypt).
    pub password_hash: String,
    #[serde(default)]
    pub autologin: bool,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct System {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// IANA zone, e.g. "Asia/Kolkata".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// e.g. "en_IN.UTF-8".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// XKB layout, e.g. "us".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Wifi {
    pub ssid: String,
    /// 64 hex chars: PBKDF2-HMAC-SHA1(passphrase, ssid, 4096, 32), as wpa_passphrase prints.
    pub psk: String,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thallium {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    /// Extra packages/apps to install after the base system.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    /// Skip the first-boot welcome screens.
    #[serde(default)]
    pub skip_setup: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Channel { Vanguard, Edgerunners }

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Theme { Dark, Light }

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Disk {
    pub mode: DiskMode,
    /// Disk to erase, e.g. "/dev/nvme0n1". Required for `erase`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Must be true for `erase` to run without the installer's final prompt.
    #[serde(default)]
    pub confirm_erase: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum DiskMode { Ask, Erase }

impl SetupConfig {
    pub fn parse(text: &str) -> Result<Self> {
        let c: Self = toml::from_str(text).context("parsing setup config")?;
        c.validate()?;
        Ok(c)
    }

    pub fn to_toml(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("# Thallium 81 unattended setup — read by the installer\n{}", toml::to_string(self)?))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "unsupported setup config version {}", self.version);
        if let Some(a) = &self.account {
            let u = a.username.as_bytes();
            ensure!(!u.is_empty() && u.len() <= 32
                && (u[0].is_ascii_lowercase() || u[0] == b'_')
                && u.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(c)),
                "username must be lowercase letters, digits, _ or -, starting with a letter");
            ensure!(a.password_hash.starts_with("$6$") || a.password_hash.starts_with("$y$"),
                "password_hash must be a $6$ or $y$ crypt hash, never a plain password");
        }
        if let Some(h) = self.system.as_ref().and_then(|s| s.hostname.as_ref()) {
            ensure!(!h.is_empty() && h.len() <= 63 && !h.starts_with('-')
                && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "hostname must be letters, digits or -");
        }
        if let Some(w) = &self.wifi {
            ensure!(!w.ssid.is_empty() && w.ssid.len() <= 32, "Wi-Fi name must be 1–32 bytes");
            ensure!(w.psk.len() == 64 && w.psk.chars().all(|c| c.is_ascii_hexdigit()),
                "wifi.psk must be the 64-hex derived key, never the passphrase");
        }
        if let Some(d) = &self.disk && d.mode == DiskMode::Erase {
            ensure!(d.target.is_some() && d.confirm_erase,
                "disk.mode = \"erase\" needs disk.target and disk.confirm_erase = true");
        }
        Ok(())
    }
}

/// Byte range of the setup partition: 32 MiB, starting at the first 1 MiB
/// boundary after the image.
pub fn partition_range(image_size: u64) -> (u64, u64) {
    let start = image_size.div_ceil(MIB) * MIB;
    (start, start + PART_SIZE)
}

/// Format the setup partition, write the config into it, then add it to the
/// image's MBR (filesystem first, so the table never points at garbage).
pub fn write(file: &File, image_size: u64, cfg: &SetupConfig) -> Result<()> {
    let text = cfg.to_toml()?;
    let (start, end) = partition_range(image_size);

    let mut mbr = [0u8; 512];
    read_at(file, &mut mbr, 0)?;
    if mbr[510..] != [0x55, 0xAA] {
        bail!("image has no MBR partition table; cannot add the setup partition");
    }
    let lba = u32::try_from(start / SECTOR)?;
    let entry = |i: usize| 446 + 16 * i;
    let slot = (0..4).find(|&i| mbr[entry(i) + 8..entry(i) + 12] == lba.to_le_bytes())
        .or_else(|| (0..4).find(|&i| mbr[entry(i)..entry(i) + 16].iter().all(|&b| b == 0)))
        .context("no free MBR slot for the setup partition")?;

    {
        let slice = fscommon::StreamSlice::new(file.try_clone()?, start, end)?;
        let mut slice = slice;
        fatfs::format_volume(&mut slice, fatfs::FormatVolumeOptions::new().volume_label(*LABEL).volume_id(volume_id()))
            .context("formatting setup partition")?;
        let fs = fatfs::FileSystem::new(slice, fatfs::FsOptions::new())?;
        let mut f = fs.root_dir().create_file(FILE_NAME)?;
        f.truncate()?;
        f.write_all(text.as_bytes())?;
        drop(f);
        fs.unmount()?;
    }

    let o = entry(slot);
    let sectors = u32::try_from((end - start) / SECTOR)?;
    let mut e = [0u8; 16];
    e[1..4].copy_from_slice(&[0xFE, 0xFF, 0xFF]); // CHS: use LBA
    e[4] = 0x0E; // FAT16 LBA
    e[5..8].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
    e[8..12].copy_from_slice(&lba.to_le_bytes());
    e[12..16].copy_from_slice(&sectors.to_le_bytes());
    mbr[o..o + 16].copy_from_slice(&e);
    write_at(file, &mbr, 0)?;
    file.sync_all()?;
    reread_partitions(file);
    Ok(())
}

/// FAT serial from the clock, so two drives never share a UUID.
fn volume_id() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u32).unwrap_or(0x5448_4C4D)
}

/// Read the config back from an image/disk that has a setup partition.
pub fn read(file: &File, image_size: u64) -> Result<SetupConfig> {
    let (start, end) = partition_range(image_size);
    let slice = fscommon::StreamSlice::new(file.try_clone()?, start, end)?;
    let fs = fatfs::FileSystem::new(slice, fatfs::FsOptions::new())?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut fs.root_dir().open_file(FILE_NAME)?, &mut text)?;
    SetupConfig::parse(&text)
}

/// Ask the kernel to pick up the new partition (block devices only; best effort).
#[cfg(target_os = "linux")]
fn reread_partitions(file: &File) {
    use std::os::fd::AsRawFd;
    const BLKRRPART: libc::c_ulong = 0x125F;
    unsafe { libc::ioctl(file.as_raw_fd(), BLKRRPART as _) };
}

#[cfg(not(target_os = "linux"))]
fn reread_partitions(_: &File) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SetupConfig {
        SetupConfig {
            version: 1,
            account: Some(Account {
                username: "neel".into(),
                full_name: Some("Neel".into()),
                password_hash: "$6$salt$abc".into(),
                autologin: true,
            }),
            system: Some(System { hostname: Some("book6".into()), timezone: Some("Asia/Kolkata".into()), ..Default::default() }),
            wifi: Some(Wifi { ssid: "home".into(), psk: "a".repeat(64) }),
            thallium: Some(Thallium { channel: Some(Channel::Vanguard), apps: vec!["thallium-music".into()], ..Default::default() }),
            disk: None,
        }
    }

    #[test]
    fn rejects_plain_secrets_and_unconfirmed_erase() {
        let mut c = sample();
        c.account.as_mut().unwrap().password_hash = "hunter2".into();
        assert!(c.validate().is_err());
        let mut c = sample();
        c.wifi.as_mut().unwrap().psk = "hunter2".into();
        assert!(c.validate().is_err());
        let mut c = sample();
        c.disk = Some(Disk { mode: DiskMode::Erase, target: Some("/dev/sda".into()), confirm_erase: false });
        assert!(c.validate().is_err());
    }

    #[test]
    fn partition_round_trip() {
        let dir = std::env::temp_dir().join(format!("tf-setup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("img");
        let image_size = 3 * MIB + 2048; // odd size: partition must start at 4 MiB
        let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).unwrap();
        f.set_len(partition_range(image_size).1).unwrap();
        let mut mbr = [0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        mbr[446 + 4] = 0x00; // slot 0 used by the "ISO"
        mbr[446 + 8..446 + 12].copy_from_slice(&0u32.to_le_bytes());
        mbr[446 + 12..446 + 16].copy_from_slice(&6150u32.to_le_bytes());
        write_at(&f, &mbr, 0).unwrap();

        write(&f, image_size, &sample()).unwrap();
        write(&f, image_size, &sample()).unwrap(); // re-run reuses the same slot
        assert_eq!(read(&f, image_size).unwrap(), sample());

        read_at(&f, &mut mbr, 0).unwrap();
        assert_eq!(mbr[446 + 16 + 4], 0x0E, "slot 1 holds the setup partition");
        assert_eq!(u32::from_le_bytes(mbr[446 + 16 + 8..446 + 16 + 12].try_into().unwrap()), 4 * 2048);
        assert!(mbr[446 + 32..446 + 48].iter().all(|&b| b == 0), "slot 2 untouched");
        std::fs::remove_dir_all(&dir).ok();
    }
}
