use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Describes one image split into fixed-size pieces, each hashed, and the
/// mirrors that serve it. Pieces are fetched from mirrors in parallel.
#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// Image file name; part files are `{name}.partNNN`.
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub piece_size: u64,
    /// sha256 of each piece, in order.
    pub pieces: Vec<String>,
    pub mirrors: Vec<Mirror>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Mirror {
    /// The full image at one URL; pieces fetched with HTTP Range (e.g. R2).
    Whole { url: String },
    /// Part files under a base URL (e.g. a GitHub release, 2 GiB file cap).
    Parts { base: String },
}

impl Mirror {
    pub fn parse(s: &str) -> Result<Self> {
        match s.split_once(':') {
            Some(("whole", url)) => Ok(Mirror::Whole { url: url.into() }),
            Some(("parts", base)) => Ok(Mirror::Parts { base: base.into() }),
            _ => bail!("mirror must be whole:<url> or parts:<base-url>, got {s}"),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Mirror::Whole { url } => url,
            Mirror::Parts { base } => base,
        }
    }
}

pub fn part_name(name: &str, index: usize) -> String {
    format!("{name}.part{index:03}")
}

impl Manifest {
    pub fn piece_range(&self, i: usize) -> (u64, u64) {
        let start = i as u64 * self.piece_size;
        (start, (start + self.piece_size).min(self.size))
    }

    pub fn validate(&self) -> Result<()> {
        if self.piece_size == 0 || self.mirrors.is_empty() {
            bail!("manifest has no piece size or no mirrors");
        }
        if self.pieces.len() as u64 != self.size.div_ceil(self.piece_size) {
            bail!("manifest piece count does not match image size");
        }
        Ok(())
    }

    pub async fn load(src: &str, client: &reqwest::Client) -> Result<Self> {
        let text = if src.starts_with("http://") || src.starts_with("https://") {
            let resp = client.get(src).send().await?;
            if crate::access::is_login_page(&resp) {
                return Err(crate::access::LoginRequired.into());
            }
            resp.error_for_status()?.text().await?
        } else {
            std::fs::read_to_string(src).with_context(|| format!("reading {src}"))?
        };
        let m: Manifest = serde_json::from_str(&text).context("parsing manifest")?;
        m.validate()?;
        Ok(m)
    }
}
