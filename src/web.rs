//! `thallium-flash web`: a local web page (like Ventoy's WebUI) served on
//! 127.0.0.1. The disk write runs as a child `thallium-flash flash` process,
//! through pkexec unless we're already root, so the browser never is.
//!
//! Every API call needs the per-run token (passed to the page in the URL
//! fragment) and a localhost Host header, so other websites open in the
//! browser can't drive it (CSRF / DNS rebinding).

use crate::access;
use crate::health;
use crate::manifest::Manifest;
use crate::setup::{Account, Channel, SetupConfig, System, Thallium, Theme, Wifi};
use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const PAGE: &str = include_str!("web/index.html");

#[derive(Serialize, Clone, Default)]
struct Job {
    running: bool,
    drive: String,
    /// download | sync | verify | setup
    phase: String,
    done: u64,
    total: u64,
    /// Set when finished: Some(true) ok, Some(false) failed.
    ok: Option<bool>,
    message: String,
}

#[derive(Serialize, Clone)]
struct ImageInfo { version: String, size: u64 }

struct Ctx {
    token: String,
    port: u16,
    manifest: String,
    image: Mutex<Option<ImageInfo>>,
    job: Mutex<Job>,
    /// Cloudflare Access token for invite-only downloads.
    token_access: Mutex<Option<String>>,
    login_needed: Mutex<bool>,
}

#[derive(Serialize)]
struct Drive { path: String, name: String, model: String, size: u64 }

#[derive(Serialize)]
struct Info {
    device: health::Device,
    drives: Vec<Drive>,
    image: Option<ImageInfo>,
    login_needed: bool,
    timezone: String,
    locale: String,
    can_flash: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SetupForm {
    username: String,
    full_name: String,
    password: String,
    autologin: bool,
    hostname: String,
    timezone: String,
    locale: String,
    keyboard: String,
    wifi_ssid: String,
    wifi_pass: String,
    channel: String,
    theme: String,
    apps: String,
    skip_setup: bool,
}

#[derive(Deserialize)]
struct FlashReq {
    drive: String,
    setup: Option<SetupForm>,
}

pub async fn serve(manifest: String, port: u16, open: bool) -> Result<()> {
    let mut raw = [0u8; 16];
    getrandom::fill(&mut raw).map_err(|e| anyhow::anyhow!("random token: {e}"))?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await
        .with_context(|| format!("binding 127.0.0.1:{port}"))?;
    let port = listener.local_addr()?.port();
    let ctx = Arc::new(Ctx {
        token: hex::encode(raw),
        port,
        manifest: manifest.clone(),
        image: Mutex::new(None),
        job: Mutex::new(Job::default()),
        token_access: Mutex::new(access::origin(&manifest).and_then(|o| access::cached_token(&o))),
        login_needed: Mutex::new(false),
    });

    // Fetch the manifest once in the background so the page can show the build.
    tokio::spawn(fetch_image(ctx.clone()));

    let app = Router::new()
        .route("/", get(page))
        .route("/api/info", get(info))
        .route("/api/job", get(job))
        .route("/api/flash", post(flash))
        .route("/api/login", post(login))
        .with_state(ctx.clone());

    let url = format!("http://127.0.0.1:{port}/#{}", ctx.token);
    eprintln!("Thallium Flash is open at:\n  {url}\nKeep this running while you flash; Ctrl+C to quit.");
    if open {
        open_browser(&url);
    }
    axum::serve(listener, app).await?;
    Ok(())
}

fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(cmd).arg(url).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// Token and Host checks for every API call.
fn guard(ctx: &Ctx, h: &HeaderMap) -> Result<(), Response> {
    let host = h.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    let host_ok = host == format!("127.0.0.1:{}", ctx.port) || host == format!("localhost:{}", ctx.port);
    let token_ok = h.get("x-token").and_then(|v| v.to_str().ok()) == Some(ctx.token.as_str());
    if host_ok && token_ok { Ok(()) } else { Err((StatusCode::FORBIDDEN, "forbidden").into_response()) }
}

async fn page() -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "no-store")], Html(PAGE))
}

async fn info(State(ctx): State<Arc<Ctx>>, h: HeaderMap) -> Response {
    if let Err(r) = guard(&ctx, &h) { return r; }
    let tz = std::fs::read_link("/etc/localtime").ok()
        .and_then(|p| p.to_string_lossy().split("zoneinfo/").nth(1).map(str::to_string))
        .unwrap_or_default();
    Json(Info {
        device: health::scan(),
        drives: drives(),
        image: ctx.image.lock().unwrap().clone(),
        login_needed: *ctx.login_needed.lock().unwrap(),
        timezone: tz,
        locale: std::env::var("LANG").unwrap_or_default(),
        can_flash: cfg!(target_os = "linux"),
    }).into_response()
}

async fn job(State(ctx): State<Arc<Ctx>>, h: HeaderMap) -> Response {
    if let Err(r) = guard(&ctx, &h) { return r; }
    Json(ctx.job.lock().unwrap().clone()).into_response()
}

async fn flash(State(ctx): State<Arc<Ctx>>, h: HeaderMap, Json(req): Json<FlashReq>) -> Response {
    if let Err(r) = guard(&ctx, &h) { return r; }
    if *ctx.login_needed.lock().unwrap() {
        return (StatusCode::FORBIDDEN, "Sign in first: this download is invite-only.").into_response();
    }
    let bad = |m: String| (StatusCode::BAD_REQUEST, m).into_response();

    // Only drives we listed ourselves (removable, whole disks) can be targets.
    let Some(drive) = drives().into_iter().find(|d| d.path == req.drive) else {
        return bad("That drive isn't a removable USB disk any more. Refresh and pick again.".into());
    };
    let setup = match req.setup.map(build_config).transpose() {
        Ok(s) => s,
        Err(e) => return bad(e),
    };
    {
        let mut j = ctx.job.lock().unwrap();
        if j.running {
            return (StatusCode::CONFLICT, "A flash is already running.").into_response();
        }
        *j = Job { running: true, drive: drive.name.clone(), phase: "start".into(), ..Default::default() };
    }
    let setup_path = match setup.map(|c| write_temp_config(&c)).transpose() {
        Ok(p) => p,
        Err(e) => {
            finish(&ctx, false, format!("Couldn't save the setup file: {e}"));
            return bad(e.to_string());
        }
    };
    let c = ctx.clone();
    std::thread::spawn(move || run_child(&c, PathBuf::from(&drive.path), setup_path));
    StatusCode::ACCEPTED.into_response()
}

fn finish(ctx: &Ctx, ok: bool, message: String) {
    let mut j = ctx.job.lock().unwrap();
    j.running = false;
    j.ok = Some(ok);
    j.message = message;
}

fn run_child(ctx: &Ctx, target: PathBuf, setup: Option<PathBuf>) {
    let exe = std::env::current_exe().unwrap_or_else(|_| "thallium-flash".into());
    let mut cmd = if is_root() { Command::new(&exe) } else {
        let mut c = Command::new("pkexec");
        c.arg(&exe);
        c
    };
    cmd.args(["flash", "--yes", "--machine", "--manifest", &ctx.manifest]);
    if let Some(s) = &setup {
        cmd.arg("--setup").arg(s);
    }
    // The root child can't reach this user's cloudflared login, so hand the
    // token over in a private file rather than on the (world-readable) cmdline.
    let token = ctx.token_access.lock().unwrap().clone();
    let token_file = match token.map(|t| write_private("access-token", t.as_bytes())).transpose() {
        Ok(p) => p,
        Err(e) => return finish(ctx, false, format!("Couldn't hand over the sign-in: {e}")),
    };
    if let Some(p) = &token_file {
        cmd.arg("--access-token-file").arg(p);
    }
    cmd.arg(&target);

    let child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
    let cleanup = || {
        for p in [&setup, &token_file].into_iter().flatten() {
            let _ = std::fs::remove_file(p);
        }
    };
    let mut child = match child {
        Ok(c) => c,
        Err(e) => { cleanup(); return finish(ctx, false, format!("Couldn't start the flasher: {e}")); }
    };
    let mut err = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || { let mut s = String::new(); let _ = err.read_to_string(&mut s); s });
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        let mut p = line.split(' ');
        if let (Some(phase), Some(done), Some(total)) = (p.next(), p.next(), p.next()) {
            let mut j = ctx.job.lock().unwrap();
            j.phase = phase.into();
            j.done = done.parse().unwrap_or(0);
            j.total = total.parse().unwrap_or(0);
        }
    }
    let status = child.wait();
    let stderr = err_thread.join().unwrap_or_default();
    cleanup();
    match status {
        Ok(s) if s.success() => finish(ctx, true, "Done. The drive checks out, so you can unplug it and boot Thallium.".into()),
        Ok(s) if s.code() == Some(126) => finish(ctx, false, "Cancelled at the password prompt. Nothing was written.".into()),
        _ => finish(ctx, false, stderr.lines().last().unwrap_or("unknown error").trim_start_matches("Error: ").to_string()),
    }
}

#[cfg(target_os = "linux")]
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(target_os = "linux"))]
fn is_root() -> bool {
    false
}

/// Removable whole disks (Linux).
fn drives() -> Vec<Drive> {
    let mut v: Vec<Drive> = std::fs::read_dir("/sys/block").into_iter().flatten().flatten().filter_map(|e| {
        let p = e.path();
        let read = |f: &str| std::fs::read_to_string(p.join(f)).unwrap_or_default().trim().to_string();
        let size = read("size").parse::<u64>().ok()? * 512;
        if read("removable") != "1" || size == 0 {
            return None;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let model = format!("{} {}", read("device/vendor"), read("device/model")).trim().to_string();
        Some(Drive { path: format!("/dev/{name}"), name, model, size })
    }).collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn opt(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Hash secrets here; only the hashes are ever written to disk.
fn build_config(f: SetupForm) -> Result<SetupConfig, String> {
    use sha_crypt::{PasswordHasher, ShaCrypt};
    let account = match opt(&f.username) {
        None => None,
        Some(username) => {
            if f.password.is_empty() {
                return Err("Set a password for the account, or clear the username.".into());
            }
            let hash = ShaCrypt::SHA512.hash_password(f.password.as_bytes()).map_err(|e| format!("hashing password: {e}"))?;
            Some(Account { username, full_name: opt(&f.full_name), password_hash: hash.to_string(), autologin: f.autologin })
        }
    };
    let wifi = match opt(&f.wifi_ssid) {
        None => None,
        Some(ssid) => {
            if !(8..=63).contains(&f.wifi_pass.len()) {
                return Err("Wi-Fi password must be 8–63 characters.".into());
            }
            let mut psk = [0u8; 32];
            pbkdf2::pbkdf2_hmac::<sha1::Sha1>(f.wifi_pass.as_bytes(), ssid.as_bytes(), 4096, &mut psk);
            Some(Wifi { ssid, psk: hex::encode(psk) })
        }
    };
    let system = System { hostname: opt(&f.hostname), timezone: opt(&f.timezone), locale: opt(&f.locale), keyboard: opt(&f.keyboard) };
    let cfg = SetupConfig {
        version: 1,
        account,
        system: (system != System::default()).then_some(system),
        wifi,
        thallium: Some(Thallium {
            channel: Some(if f.channel == "edgerunners" { Channel::Edgerunners } else { Channel::Vanguard }),
            theme: Some(if f.theme == "light" { Theme::Light } else { Theme::Dark }),
            apps: f.apps.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            skip_setup: f.skip_setup,
        }),
        disk: None,
    };
    cfg.validate().map_err(|e| e.to_string())?;
    Ok(cfg)
}

/// Private temp file (0600) handed to the child, removed after the run.
fn write_temp_config(cfg: &SetupConfig) -> Result<PathBuf> {
    write_private("setup.toml", cfg.to_toml()?.as_bytes())
}

/// New 0600 file in the temp dir; create_new so an existing (possibly
/// planted) file with looser permissions is never reused.
fn write_private(name: &str, data: &[u8]) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("thallium-flash-{}-{name}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    std::io::Write::write_all(&mut o.open(&path)?, data)?;
    Ok(path)
}

/// Load the manifest for the page header; flags a needed sign-in.
async fn fetch_image(ctx: Arc<Ctx>) {
    let token = ctx.token_access.lock().unwrap().clone();
    let Ok(client) = access::client(token.as_deref()) else { return };
    match Manifest::load(&ctx.manifest, &client).await {
        Ok(m) => {
            *ctx.image.lock().unwrap() = Some(ImageInfo { version: m.version, size: m.size });
            *ctx.login_needed.lock().unwrap() = false;
        }
        Err(e) if e.is::<access::LoginRequired>() => *ctx.login_needed.lock().unwrap() = true,
        Err(_) => {}
    }
}

/// Run the cloudflared email-code sign-in (opens the browser) and wait.
async fn login(State(ctx): State<Arc<Ctx>>, h: HeaderMap) -> Response {
    if let Err(r) = guard(&ctx, &h) { return r; }
    let Some(origin) = access::origin(&ctx.manifest) else {
        return (StatusCode::BAD_REQUEST, "No download host to sign in to.").into_response();
    };
    match tokio::task::spawn_blocking(move || access::login(&origin)).await {
        Ok(Ok(t)) => {
            *ctx.token_access.lock().unwrap() = Some(t);
            fetch_image(ctx.clone()).await;
            if *ctx.login_needed.lock().unwrap() {
                return (StatusCode::FORBIDDEN, "Signed in, but this email isn't on the invite list.").into_response();
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Err(e)) => (StatusCode::BAD_GATEWAY, format!("{e:#}")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
