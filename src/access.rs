//! Cloudflare Access (email one-time-code login) in front of the download
//! host. We don't speak Access's login protocol ourselves: `cloudflared`
//! opens the browser login and caches a per-app token, which we then send as
//! the `cf-access-token` header on every request.

use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};

/// Returned (inside anyhow) when the download host wants an Access login.
#[derive(Debug)]
pub struct LoginRequired;

impl std::fmt::Display for LoginRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("sign-in required: this download is limited to invited people")
    }
}

impl std::error::Error for LoginRequired {}

/// True when a response came back from Access instead of the file.
pub fn is_login_page(resp: &reqwest::Response) -> bool {
    resp.url().host_str().is_some_and(|h| h.ends_with("cloudflareaccess.com"))
        || resp.status() == reqwest::StatusCode::FORBIDDEN && resp.headers().contains_key("cf-access-domain")
}

/// "https://dl.example.com/x/y.json" -> "https://dl.example.com"
pub fn origin(url: &str) -> Option<String> {
    let u = reqwest::Url::parse(url).ok()?;
    Some(format!("{}://{}", u.scheme(), u.host_str()?))
}

/// Cached token for `origin`, if cloudflared has one.
pub fn cached_token(origin: &str) -> Option<String> {
    let out = Command::new("cloudflared").args(["access", "token", &format!("-app={origin}")])
        .stderr(Stdio::null()).output().ok()?;
    let t = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && t.split('.').count() == 3).then_some(t)
}

/// Open the browser email-code login and wait for it; returns the token.
pub fn login(origin: &str) -> Result<String> {
    let status = Command::new("cloudflared").args(["access", "login", origin])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status()
        .context("signing in needs cloudflared: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/")?;
    if !status.success() {
        bail!("sign-in did not finish");
    }
    cached_token(origin).context("signed in, but cloudflared returned no token")
}

/// HTTP client that sends the Access token (if any) with every request.
pub fn client(token: Option<&str>) -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(t) = token {
        headers.insert("cf-access-token", t.parse().context("bad access token")?);
    }
    Ok(reqwest::Client::builder()
        .user_agent(concat!("thallium-flash/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(30))
        .default_headers(headers)
        .build()?)
}
