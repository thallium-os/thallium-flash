mod flash;
mod manifest;
mod split;
mod target;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

const DEFAULT_MANIFEST: &str =
    "https://github.com/thallium-os/thallium-flash/releases/latest/download/manifest.json";

/// Parallel, multi-mirror image downloader that writes straight to USB.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Cut an ISO into hashed part files plus manifest.json (release side)
    Split {
        iso: PathBuf,
        #[arg(short, long, default_value = "dist")]
        out: PathBuf,
        #[arg(long)]
        version: String,
        /// Piece size in MiB (multiple of 4)
        #[arg(long, default_value_t = 256)]
        piece_mb: u64,
        /// whole:<url-of-full-iso> or parts:<base-url-of-part-files>; repeatable
        #[arg(short, long = "mirror", required = true)]
        mirrors: Vec<String>,
    },
    /// Download every piece in parallel and write it to a USB disk or file
    Flash {
        /// manifest.json URL or path
        #[arg(short, long, default_value = DEFAULT_MANIFEST)]
        manifest: String,
        /// /dev/sdX (whole disk) or an output file path
        target: PathBuf,
        /// Parallel piece downloads
        #[arg(short, long, default_value_t = 8)]
        jobs: usize,
        /// Skip the erase confirmation
        #[arg(long)]
        yes: bool,
        /// Allow non-removable disks
        #[arg(long)]
        force: bool,
        /// Keep pieces already on the target that hash correctly
        #[arg(long)]
        resume: bool,
        /// Skip the read-back verify
        #[arg(long)]
        no_verify: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Split { iso, out, version, piece_mb, mirrors } => split::run(&iso, &out, &version, piece_mb, &mirrors),
        Cmd::Flash { manifest, target, jobs, yes, force, resume, no_verify } => {
            flash::run(flash::Opts { manifest, target, jobs, yes, force, resume, no_verify }).await
        }
    }
}
