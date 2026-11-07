use crate::manifest::{Manifest, Mirror, part_name};
use crate::target::{self, FLUSH};
use anyhow::{Context, Result, bail};
use futures_util::{StreamExt, TryStreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub struct Opts {
    pub manifest: String,
    pub target: PathBuf,
    pub jobs: usize,
    pub yes: bool,
    pub force: bool,
    pub resume: bool,
    pub no_verify: bool,
}

struct Ctx {
    m: Manifest,
    file: Arc<File>,
    client: reqwest::Client,
    done: AtomicU64,
    pb: ProgressBar,
}

pub async fn run(o: Opts) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("thallium-flash/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let m = Manifest::load(&o.manifest, &client).await?;
    eprintln!("{} {} — {:.2} GB, {} pieces, {} mirrors",
        m.name, m.version, m.size as f64 / 1e9, m.pieces.len(), m.mirrors.len());

    let file = Arc::new(target::open(&o.target, m.size, o.force, o.yes, o.resume)?);

    let mut todo: Vec<usize> = (0..m.pieces.len()).collect();
    if o.resume {
        let mut left = Vec::new();
        for i in todo {
            if hash_region(&file, m.piece_range(i)).await? != m.pieces[i] {
                left.push(i);
            }
        }
        eprintln!("resume: {} of {} pieces already good", m.pieces.len() - left.len(), m.pieces.len());
        todo = left;
    }

    let total: u64 = todo.iter().map(|&i| { let (a, b) = m.piece_range(i); b - a }).sum();
    let pb = bar(total, "download");
    let ctx = Arc::new(Ctx { m, file, client, done: AtomicU64::new(0), pb });

    stream::iter(todo)
        .map(|i| piece(ctx.clone(), i))
        .buffer_unordered(o.jobs.max(1))
        .try_collect::<()>()
        .await?;
    ctx.pb.finish();

    let f = ctx.file.clone();
    tokio::task::spawn_blocking(move || { f.sync_all()?; target::drop_cache(&f); anyhow::Ok(()) }).await??;

    if o.no_verify {
        eprintln!("written (verify skipped)");
        return Ok(());
    }
    let got = verify(&ctx.file, ctx.m.size).await?;
    if got != ctx.m.sha256 {
        bail!("verify FAILED: target sha256 {got}, expected {}", ctx.m.sha256);
    }
    eprintln!("verified: sha256 {got}");
    Ok(())
}

fn bar(len: u64, msg: &'static str) -> ProgressBar {
    let pb = ProgressBar::new(len).with_message(msg);
    pb.set_style(ProgressStyle::with_template(
        "{msg:8} [{bar:40}] {bytes}/{total_bytes} {binary_bytes_per_sec} eta {eta}",
    ).unwrap().progress_chars("=> "));
    pb
}

/// Fetch one piece, rotating through mirrors (starting at a different mirror
/// per piece so load spreads across all of them) until its hash matches.
async fn piece(ctx: Arc<Ctx>, i: usize) -> Result<()> {
    let n = ctx.m.mirrors.len();
    let tries = (n * 2).max(3);
    for attempt in 0..tries {
        let mirror = &ctx.m.mirrors[(i + attempt) % n];
        let mut written = 0;
        let res = fetch(&ctx, mirror, i, &mut written).await;
        match res {
            Ok(h) if h == ctx.m.pieces[i] => return Ok(()),
            Ok(_) => warn(&ctx.pb, format!("piece {i}: hash mismatch from {}", mirror.label())),
            Err(e) => warn(&ctx.pb, format!("piece {i}: {} — {e:#}", mirror.label())),
        }
        let pos = ctx.done.fetch_sub(written, Ordering::Relaxed) - written;
        ctx.pb.set_position(pos);
        tokio::time::sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
    }
    bail!("piece {i} failed on every mirror")
}

/// Stream a piece straight into the target at its offset, hashing as it goes.
async fn fetch(ctx: &Ctx, mirror: &Mirror, i: usize, written: &mut u64) -> Result<String> {
    let (start, end) = ctx.m.piece_range(i);
    let len = end - start;
    let req = match mirror {
        Mirror::Whole { url } => ctx.client.get(url)
            .header(reqwest::header::RANGE, format!("bytes={start}-{}", end - 1)),
        Mirror::Parts { base } => ctx.client.get(format!("{}/{}",
            base.trim_end_matches('/'), part_name(&ctx.m.name, i))),
    };
    let resp = req.send().await?.error_for_status()?;
    if matches!(mirror, Mirror::Whole { .. }) && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        bail!("server ignored Range (status {})", resp.status());
    }
    if let Some(cl) = resp.content_length() && cl != len {
        bail!("expected {len} bytes, server sends {cl}");
    }

    let mut h = Sha256::new();
    let mut buf: Vec<u8> = Vec::with_capacity(FLUSH * 2);
    let mut off = start;
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.context("download interrupted")?;
        if off + (buf.len() + chunk.len()) as u64 > end {
            bail!("server sent more than {len} bytes");
        }
        h.update(&chunk);
        buf.extend_from_slice(&chunk);
        while buf.len() >= FLUSH {
            let rest = buf.split_off(FLUSH);
            off = flush(ctx, std::mem::replace(&mut buf, rest), off, written).await?;
        }
    }
    if !buf.is_empty() {
        off = flush(ctx, buf, off, written).await?;
    }
    if off != end {
        bail!("short read: got {} of {len} bytes", off - start);
    }
    Ok(hex::encode(h.finalize()))
}

async fn flush(ctx: &Ctx, buf: Vec<u8>, off: u64, written: &mut u64) -> Result<u64> {
    let n = buf.len() as u64;
    let f = ctx.file.clone();
    tokio::task::spawn_blocking(move || target::write_at(&f, &buf, off)).await??;
    *written += n;
    ctx.pb.set_position(ctx.done.fetch_add(n, Ordering::Relaxed) + n);
    Ok(off + n)
}

async fn hash_region(file: &Arc<File>, (start, end): (u64, u64)) -> Result<String> {
    let f = file.clone();
    tokio::task::spawn_blocking(move || {
        let mut h = Sha256::new();
        let mut buf = vec![0u8; FLUSH];
        let mut off = start;
        while off < end {
            let n = (end - off).min(FLUSH as u64) as usize;
            target::read_at(&f, &mut buf[..n], off)?;
            h.update(&buf[..n]);
            off += n as u64;
        }
        Ok(hex::encode(h.finalize()))
    }).await?
}

async fn verify(file: &Arc<File>, size: u64) -> Result<String> {
    let f = file.clone();
    let pb = bar(size, "verify");
    let pb2 = pb.clone();
    let got = tokio::task::spawn_blocking(move || {
        let mut h = Sha256::new();
        let mut buf = vec![0u8; FLUSH];
        let mut off = 0;
        while off < size {
            let n = (size - off).min(FLUSH as u64) as usize;
            target::read_at(&f, &mut buf[..n], off)?;
            h.update(&buf[..n]);
            off += n as u64;
            pb2.set_position(off);
        }
        anyhow::Ok(hex::encode(h.finalize()))
    }).await??;
    pb.finish();
    Ok(got)
}

/// indicatif drops println when stderr is not a terminal; fall back to eprintln.
fn warn(pb: &ProgressBar, msg: String) {
    if pb.is_hidden() { eprintln!("{msg}") } else { pb.println(msg) }
}
