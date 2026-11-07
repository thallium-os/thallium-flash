use anyhow::{Context, Result, bail};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Write granularity. Every write starts on a 4 MiB boundary so raw devices
/// that require sector-aligned I/O are happy.
pub const FLUSH: usize = 4 * 1024 * 1024;

/// Open the flash target: a regular file (sized to the image) or, on Linux,
/// a whole removable block device after safety checks and confirmation.
pub fn open(path: &Path, size: u64, force: bool, yes: bool, resume: bool) -> Result<File> {
    if is_block_device(path) {
        check_device(path, size, force, yes)?;
        return OpenOptions::new().read(true).write(true).open(path)
            .with_context(|| format!("opening {} (run as root?)", path.display()));
    }
    if path.exists() && !path.is_file() {
        bail!("{} is not a regular file or a supported block device", path.display());
    }
    let f = OpenOptions::new().read(true).write(true).create(true).truncate(!resume).open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    f.set_len(size)?;
    Ok(f)
}

#[cfg(unix)]
fn is_block_device(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(path).map(|m| m.file_type().is_block_device()).unwrap_or(false)
}

#[cfg(not(unix))]
fn is_block_device(_: &Path) -> bool {
    false
}

#[cfg(target_os = "linux")]
fn check_device(path: &Path, size: u64, force: bool, yes: bool) -> Result<()> {
    let name = path.file_name().context("bad device path")?.to_string_lossy().into_owned();
    let sys = Path::new("/sys/block").join(&name);
    if !sys.exists() {
        bail!("{} is not a whole disk (pass /dev/sdX, not a partition)", path.display());
    }
    let read = |f: &str| std::fs::read_to_string(sys.join(f)).unwrap_or_default().trim().to_string();
    if read("removable") != "1" && !force {
        bail!("{} is not a removable disk; refusing (use --force if you are sure)", path.display());
    }
    let dev = format!("/dev/{name}");
    let mounts = std::fs::read_to_string("/proc/mounts")?;
    if let Some(m) = mounts.lines().find(|l| l.starts_with(&dev)) {
        bail!("{dev} is mounted ({}); unmount it first", m.split(' ').next().unwrap_or(""));
    }
    let bytes = read("size").parse::<u64>().unwrap_or(0) * 512;
    if bytes < size {
        bail!("{dev} is {} but the image needs {}", gb(bytes), gb(size));
    }
    if !yes {
        eprint!("ERASE {dev} ({} {}, {})? Type YES: ", read("device/vendor"), read("device/model"), gb(bytes));
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if line.trim() != "YES" {
            bail!("aborted");
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn check_device(path: &Path, _: u64, _: bool, _: bool) -> Result<()> {
    bail!("flashing devices is Linux-only for now; write to a file instead of {}", path.display())
}

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1e9)
}

/// Drop cached pages so the read-back verify hits the device, not RAM.
#[cfg(target_os = "linux")]
pub fn drop_cache(f: &File) {
    use std::os::fd::AsRawFd;
    unsafe { libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
}

#[cfg(not(target_os = "linux"))]
pub fn drop_cache(_: &File) {}

#[cfg(unix)]
pub fn write_at(f: &File, buf: &[u8], off: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::write_all_at(f, buf, off)
}

#[cfg(unix)]
pub fn read_at(f: &File, buf: &mut [u8], off: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(f, buf, off)
}

#[cfg(windows)]
pub fn write_at(f: &File, mut buf: &[u8], mut off: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = f.seek_write(buf, off)?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        buf = &buf[n..];
        off += n as u64;
    }
    Ok(())
}

#[cfg(windows)]
pub fn read_at(f: &File, mut buf: &mut [u8], mut off: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = f.seek_read(buf, off)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf = &mut buf[n..];
        off += n as u64;
    }
    Ok(())
}
