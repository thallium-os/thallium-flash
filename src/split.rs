use crate::manifest::{Manifest, Mirror, part_name};
use crate::target::FLUSH;
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Cut an image into part files and write manifest.json next to them.
pub fn run(iso: &Path, out: &Path, version: &str, piece_mb: u64, mirrors: &[String]) -> Result<()> {
    let piece_size = piece_mb * 1024 * 1024;
    if piece_size == 0 || piece_size % FLUSH as u64 != 0 {
        bail!("piece size must be a multiple of 4 MiB");
    }
    let mirrors = mirrors.iter().map(|m| Mirror::parse(m)).collect::<Result<Vec<_>>>()?;
    let name = iso.file_name().context("image path has no file name")?.to_string_lossy().into_owned();
    std::fs::create_dir_all(out)?;

    let mut src = File::open(iso).with_context(|| format!("opening {}", iso.display()))?;
    let size = src.metadata()?.len();
    let mut whole = Sha256::new();
    let mut pieces = Vec::new();
    let mut buf = vec![0u8; FLUSH];

    for i in 0..size.div_ceil(piece_size) as usize {
        let part = out.join(part_name(&name, i));
        let mut dst = File::create(&part).with_context(|| format!("creating {}", part.display()))?;
        let mut h = Sha256::new();
        let mut left = piece_size.min(size - i as u64 * piece_size);
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            src.read_exact(&mut buf[..n])?;
            h.update(&buf[..n]);
            whole.update(&buf[..n]);
            dst.write_all(&buf[..n])?;
            left -= n as u64;
        }
        pieces.push(hex::encode(h.finalize()));
        eprintln!("{}", part.display());
    }

    let m = Manifest {
        version: version.into(),
        name,
        size,
        sha256: hex::encode(whole.finalize()),
        piece_size,
        pieces,
        mirrors,
    };
    let path = out.join("manifest.json");
    std::fs::write(&path, serde_json::to_string_pretty(&m)?)?;
    eprintln!("{} ({} pieces, sha256 {})", path.display(), m.pieces.len(), m.sha256);
    Ok(())
}
