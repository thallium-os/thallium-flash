use thallium_flash::{flash, split, web};

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

const DEFAULT_MANIFEST: &str =
    "https://dl.thallium81.dev/manifest.json";

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
        /// Machine-readable progress on stdout (used by the GUI)
        #[arg(long)]
        machine: bool,
        /// thallium-setup.toml with unattended install answers (adds a THALLIUMCFG partition)
        #[arg(long)]
        setup: Option<PathBuf>,
        /// File with a Cloudflare Access token (for invite-only downloads when run as root)
        #[arg(long)]
        access_token_file: Option<PathBuf>,
    },
    /// Open the flasher as a local web page (like Ventoy's WebUI)
    Web {
        /// manifest.json URL or path
        #[arg(short, long, default_value = DEFAULT_MANIFEST)]
        manifest: String,
        /// Port on 127.0.0.1 (0 = pick a free one)
        #[arg(short, long, default_value_t = 0)]
        port: u16,
        /// Print the link instead of opening the browser
        #[arg(long)]
        no_open: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Split { iso, out, version, piece_mb, mirrors } => split::run(&iso, &out, &version, piece_mb, &mirrors),
        Cmd::Flash { manifest, target, jobs, yes, force, resume, no_verify, machine, setup, access_token_file } => {
            flash::run(flash::Opts { manifest, target, jobs, yes, force, resume, no_verify, machine, setup, access_token_file }).await
        }
        Cmd::Web { manifest, port, no_open } => web::serve(manifest, port, !no_open).await,
    }
}
