//! "Can this PC run Thallium 81?" checks for the computer the tool runs on,
//! read straight from /proc and /sys. Thresholds are the stated minimums.

use serde::Serialize;

const MIN_RAM: f64 = 3.5e9; // "4 GB": firmware reserves a little
const REC_RAM: f64 = 7.0e9; // "8 GB"
const MIN_DISK: f64 = 30.0e9; // "32 GB"
const REC_DISK: f64 = 60.0e9; // "64 GB"

#[derive(Serialize, Clone, Copy, PartialEq, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum Level { Ok, Note, Fail, Info }

#[derive(Serialize)]
pub struct Check {
    pub label: &'static str,
    pub value: String,
    pub note: String,
    pub level: Level,
}

#[derive(Serialize)]
pub struct Device {
    pub name: String,
    pub os: String,
    pub checks: Vec<Check>,
    /// Worst non-info level.
    pub verdict: Level,
}

fn gb(b: f64) -> String {
    format!("{:.0} GB", b / 1e9)
}

fn c(label: &'static str, value: impl Into<String>, note: impl Into<String>, level: Level) -> Check {
    Check { label, value: value.into(), note: note.into(), level }
}

fn read(p: &str) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| l.strip_prefix(key)?.trim_start().strip_prefix(':').or_else(|| l.strip_prefix(key)?.strip_prefix('=')).map(str::trim))
}

pub fn scan() -> Device {
    let mut checks = Vec::new();

    let arch = std::env::consts::ARCH;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let cpuinfo = read("/proc/cpuinfo");
    let brand = field(&cpuinfo, "model name").unwrap_or(arch).to_string();
    checks.push(match (arch, threads) {
        ("x86_64", t) if t >= 2 => c("Processor", brand, format!("64-bit x86, {t} threads"), Level::Ok),
        ("x86_64", _) => c("Processor", brand, "Single-threaded CPUs are too slow for the desktop", Level::Fail),
        (a, _) => c("Processor", brand, format!("Thallium 81 is built for x86-64 only; this is {a}"), Level::Fail),
    });

    let ram = field(&read("/proc/meminfo"), "MemTotal")
        .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(0.0) * 1024.0;
    checks.push(match ram {
        r if r == 0.0 => c("Memory", "Unknown", "Couldn't read installed memory", Level::Info),
        r if r < MIN_RAM => c("Memory", gb(r), "Needs at least 4 GB", Level::Fail),
        r if r < REC_RAM => c("Memory", gb(r), "Runs, but 8 GB is recommended", Level::Note),
        r => c("Memory", gb(r), "Meets the 8 GB recommendation", Level::Ok),
    });

    let disk = largest_internal_disk();
    checks.push(match disk {
        d if d == 0.0 => c("Storage", "Unknown", "Couldn't find an internal disk", Level::Info),
        d if d < MIN_DISK => c("Storage", gb(d), "Needs a disk of at least 32 GB", Level::Fail),
        d if d < REC_DISK => c("Storage", gb(d), "Fits, but 64 GB leaves room for apps", Level::Note),
        d => c("Storage", gb(d), "Largest internal disk, plenty of room", Level::Ok),
    });

    checks.push(if !std::path::Path::new("/sys/firmware/efi").exists() {
        c("Firmware", "Legacy BIOS", "Boots fine; UEFI is recommended when available", Level::Note)
    } else if std::fs::read("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c")
        .ok().and_then(|v| v.get(4).copied()) == Some(1)
    {
        c("Firmware", "UEFI, Secure Boot on", "If the USB drive won't start, turn Secure Boot off in firmware settings", Level::Info)
    } else {
        c("Firmware", "UEFI", "Boots the Thallium USB drive directly", Level::Ok)
    });

    let mut gpus: Vec<&str> = std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten()
        .filter(|e| { let n = e.file_name().to_string_lossy().into_owned(); n.starts_with("card") && !n.contains('-') })
        .map(|e| match read(&format!("{}/device/vendor", e.path().display())).trim() {
            "0x8086" => "Intel", "0x1002" => "AMD", "0x10de" => "NVIDIA", _ => "Other",
        })
        .collect();
    gpus.sort();
    gpus.dedup();
    checks.push(if gpus.is_empty() {
        c("Graphics", "Unknown", "Couldn't identify the graphics card", Level::Info)
    } else if gpus.contains(&"NVIDIA") {
        c("Graphics", gpus.join(" + "), "NVIDIA works but needs its proprietary driver; expect some rough edges", Level::Note)
    } else {
        c("Graphics", gpus.join(" + "), "Open drivers ship with Thallium", Level::Ok)
    });

    let verdict = checks.iter().map(|c| c.level).filter(|l| *l != Level::Info)
        .fold(Level::Ok, |a, b| if b > a { b } else { a });
    let os = read("/etc/os-release");
    Device {
        name: read("/proc/sys/kernel/hostname").trim().to_string(),
        os: field(&os, "PRETTY_NAME").unwrap_or(std::env::consts::OS).trim_matches('"').to_string(),
        checks,
        verdict,
    }
}

fn largest_internal_disk() -> f64 {
    std::fs::read_dir("/sys/block").into_iter().flatten().flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            !["loop", "ram", "zram", "dm-", "sr", "md", "fd"].iter().any(|p| n.starts_with(p))
        })
        .filter(|e| read(&format!("{}/removable", e.path().display())).trim() == "0")
        .filter_map(|e| read(&format!("{}/size", e.path().display())).trim().parse::<f64>().ok())
        .map(|sectors| sectors * 512.0)
        .fold(0.0, f64::max)
}
