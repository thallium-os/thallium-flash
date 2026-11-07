//! "Can this PC run Thallium 81?" checks for the computer the app runs on.
//! Thresholds are Thallium's stated minimums; keep them in sync with the docs.

pub const MIN_RAM: f64 = 3.5e9; // "4 GB": firmware reserves a little
pub const REC_RAM: f64 = 7.0e9; // "8 GB"
pub const MIN_DISK: f64 = 30.0e9; // "32 GB"
pub const REC_DISK: f64 = 60.0e9; // "64 GB"

#[derive(Clone, Copy, PartialEq)]
pub enum Level { Ok = 0, Warn = 1, Fail = 2, Info = 3 }

pub struct Check {
    pub label: &'static str,
    pub value: String,
    pub note: String,
    pub level: Level,
}

pub struct Device {
    pub name: String,
    pub os: String,
    pub checks: Vec<Check>,
}

fn gb(b: f64) -> String {
    format!("{:.0} GB", b / 1e9)
}

fn check(label: &'static str, value: String, note: impl Into<String>, level: Level) -> Check {
    Check { label, value, note: note.into(), level }
}

pub fn scan() -> Device {
    use sysinfo::{RefreshKind, System};
    let sys = System::new_with_specifics(RefreshKind::everything()).ok();
    let mut checks = Vec::new();

    // Processor
    let arch = std::env::consts::ARCH;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let brand = sys.as_ref().and_then(|s| s.cpus().first().map(|c| c.brand().trim().to_string()))
        .filter(|b| !b.is_empty()).unwrap_or_else(|| arch.to_string());
    checks.push(match (arch, threads) {
        ("x86_64", t) if t >= 2 => check("PROCESSOR", brand, format!("64-bit x86, {t} threads"), Level::Ok),
        ("x86_64", _) => check("PROCESSOR", brand, "Single-threaded CPUs are too slow for the desktop", Level::Fail),
        (a, _) => check("PROCESSOR", brand, format!("Thallium 81 is built for x86-64 only; this is {a}"), Level::Fail),
    });

    // Memory
    let ram = sys.as_ref().map(|s| s.total_memory() as f64).unwrap_or(0.0);
    checks.push(if ram == 0.0 {
        check("MEMORY", "Unknown".into(), "Couldn't read installed memory", Level::Info)
    } else if ram < MIN_RAM {
        check("MEMORY", gb(ram), "Needs at least 4 GB", Level::Fail)
    } else if ram < REC_RAM {
        check("MEMORY", gb(ram), "Runs, but 8 GB is recommended", Level::Warn)
    } else {
        check("MEMORY", gb(ram), "Meets the 8 GB recommendation", Level::Ok)
    });

    // Storage
    let disk = largest_internal_disk();
    checks.push(if disk == 0.0 {
        check("STORAGE", "Unknown".into(), "Couldn't find an internal disk", Level::Info)
    } else if disk < MIN_DISK {
        check("STORAGE", gb(disk), "Needs a disk of at least 32 GB", Level::Fail)
    } else if disk < REC_DISK {
        check("STORAGE", gb(disk), "Fits, but 64 GB leaves room for apps", Level::Warn)
    } else {
        check("STORAGE", gb(disk), "Largest internal disk, plenty of room", Level::Ok)
    });

    // Firmware
    checks.push(firmware());

    // Graphics
    checks.push(graphics());

    Device {
        name: System::host_name().unwrap_or_else(|_| "This PC".into()),
        os: System::long_os_version().unwrap_or_else(|_| std::env::consts::OS.into()),
        checks,
    }
}

#[cfg(target_os = "linux")]
fn largest_internal_disk() -> f64 {
    std::fs::read_dir("/sys/block").into_iter().flatten().flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            !["loop", "ram", "zram", "dm-", "sr", "md", "fd"].iter().any(|p| n.starts_with(p))
        })
        .filter(|e| std::fs::read_to_string(e.path().join("removable")).map(|s| s.trim() == "0").unwrap_or(false))
        .filter_map(|e| std::fs::read_to_string(e.path().join("size")).ok()?.trim().parse::<f64>().ok())
        .map(|sectors| sectors * 512.0)
        .fold(0.0, f64::max)
}

#[cfg(not(target_os = "linux"))]
fn largest_internal_disk() -> f64 {
    sysinfo::Disks::new_with_refreshed_list().map(|d| {
        d.iter().filter(|d| !d.is_removable()).map(|d| d.total_space() as f64).fold(0.0, f64::max)
    }).unwrap_or(0.0)
}

#[cfg(target_os = "linux")]
fn firmware() -> Check {
    if !std::path::Path::new("/sys/firmware/efi").exists() {
        return check("FIRMWARE", "Legacy BIOS".into(), "Boots fine; UEFI is recommended when available", Level::Warn);
    }
    let sb = std::fs::read("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c")
        .ok().and_then(|v| v.get(4).copied());
    match sb {
        Some(1) => check("FIRMWARE", "UEFI · Secure Boot on".into(),
            "If the USB drive won't start, turn Secure Boot off in firmware settings", Level::Info),
        _ => check("FIRMWARE", "UEFI".into(), "Boots the Thallium USB drive directly", Level::Ok),
    }
}

#[cfg(not(target_os = "linux"))]
fn firmware() -> Check {
    check("FIRMWARE", "Not checked".into(), "Only checked when running on Linux; both UEFI and BIOS boot", Level::Info)
}

#[cfg(target_os = "linux")]
fn graphics() -> Check {
    let mut vendors: Vec<&str> = std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten()
        .filter(|e| { let n = e.file_name().to_string_lossy().into_owned(); n.starts_with("card") && !n.contains('-') })
        .filter_map(|e| std::fs::read_to_string(e.path().join("device/vendor")).ok())
        .map(|v| match v.trim() { "0x8086" => "Intel", "0x1002" => "AMD", "0x10de" => "NVIDIA", _ => "Other" })
        .collect();
    vendors.sort();
    vendors.dedup();
    let list = if vendors.is_empty() { "Unknown".to_string() } else { vendors.join(" + ") };
    if vendors.contains(&"NVIDIA") {
        check("GRAPHICS", list, "NVIDIA works but needs its proprietary driver; expect some rough edges", Level::Warn)
    } else if vendors.is_empty() {
        check("GRAPHICS", list, "Couldn't identify the graphics card", Level::Info)
    } else {
        check("GRAPHICS", list, "Open drivers ship with Thallium", Level::Ok)
    }
}

#[cfg(not(target_os = "linux"))]
fn graphics() -> Check {
    check("GRAPHICS", "Not checked".into(), "Only checked when running on Linux", Level::Info)
}
