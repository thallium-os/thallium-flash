//! Thin GUI over the `thallium-flash` CLI. It runs the CLI as a child
//! process (through pkexec on Linux, so only the disk write runs as root)
//! and turns its `--machine` progress lines into a progress bar.
//! The optional Customize section writes a hashed thallium-setup.toml that
//! the CLI puts on a THALLIUMCFG partition for an unattended install.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use thallium_flash::setup::{Account, Channel, SetupConfig, System, Thallium, Theme, Wifi};

slint::slint! {
    import { Button, CheckBox, ComboBox, LineEdit, ProgressIndicator, ScrollView } from "std-widgets.slint";

    component Field inherits HorizontalLayout {
        in property <string> label;
        spacing: 10px;
        Text { text: label; width: 110px; vertical-alignment: center; opacity: 0.8; }
        @children
    }

    export component App inherits Window {
        title: "Thallium Flash";
        preferred-width: 520px;
        in property <[string]> drives;
        in-out property <int> drive-index: 0;
        in property <float> progress;
        in property <string> status: "Pick a USB drive. Everything on it will be erased.";
        in property <bool> busy;
        in property <bool> can-customize;
        in-out property <bool> customize;

        in-out property <string> username;
        in-out property <string> full-name;
        in-out property <string> password;
        in-out property <bool> autologin;
        in-out property <string> hostname;
        in-out property <string> timezone;
        in-out property <string> locale;
        in-out property <string> keyboard;
        in-out property <string> wifi-ssid;
        in-out property <string> wifi-pass;
        in-out property <int> channel: 0;
        in-out property <int> theme: 0;
        in-out property <string> apps;
        in-out property <bool> skip-setup;

        callback flash(int);
        callback refresh();

        VerticalLayout {
            padding: 28px;
            spacing: 14px;
            Text { text: "Thallium 81"; font-size: 26px; font-weight: 700; }
            Text {
                text: "Downloads the latest ISO from every mirror at once and writes it straight to your drive.";
                wrap: word-wrap;
                opacity: 0.7;
            }
            HorizontalLayout {
                spacing: 8px;
                ComboBox {
                    model: drives;
                    current-index <=> drive-index;
                    enabled: !busy && drives.length > 0;
                    horizontal-stretch: 1;
                }
                Button { text: "Refresh"; enabled: !busy; clicked => { refresh(); } }
            }
            if can-customize: CheckBox {
                text: "Customize the install (answers the installer's questions for you)";
                checked <=> customize;
                enabled: !busy;
            }
            if can-customize && customize: ScrollView {
                height: 300px;
                enabled: !busy;
                VerticalLayout {
                    spacing: 8px;
                    padding-right: 12px;
                    Text { text: "Leave anything blank and the installer asks for it."; opacity: 0.6; font-size: 12px; }
                    Field { label: "Username"; LineEdit { text <=> username; placeholder-text: "e.g. neel"; } }
                    Field { label: "Full name"; LineEdit { text <=> full-name; } }
                    Field { label: "Password"; LineEdit { text <=> password; input-type: InputType.password; placeholder-text: "stored only as a hash"; } }
                    CheckBox { text: "Log in automatically"; checked <=> autologin; }
                    Field { label: "Computer name"; LineEdit { text <=> hostname; placeholder-text: "e.g. thallium-laptop"; } }
                    Field { label: "Time zone"; LineEdit { text <=> timezone; placeholder-text: "e.g. Asia/Kolkata"; } }
                    Field { label: "Language"; LineEdit { text <=> locale; placeholder-text: "e.g. en_IN.UTF-8"; } }
                    Field { label: "Keyboard"; LineEdit { text <=> keyboard; placeholder-text: "e.g. us"; } }
                    Field { label: "Wi-Fi name"; LineEdit { text <=> wifi-ssid; } }
                    Field { label: "Wi-Fi password"; LineEdit { text <=> wifi-pass; input-type: InputType.password; placeholder-text: "stored only as the derived key"; } }
                    Field { label: "Channel"; ComboBox { model: ["Vanguard", "Edge Runners"]; current-index <=> channel; } }
                    Field { label: "Theme"; ComboBox { model: ["Dark", "Light"]; current-index <=> theme; } }
                    Field { label: "Extra apps"; LineEdit { text <=> apps; placeholder-text: "comma separated package names"; } }
                    CheckBox { text: "Skip the welcome screens on first boot"; checked <=> skip-setup; }
                    Text {
                        text: "This file stays readable on the USB drive. The disk to install to is still chosen in the installer.";
                        wrap: word-wrap; opacity: 0.6; font-size: 12px;
                    }
                }
            }
            ProgressIndicator { progress: progress; height: 6px; }
            Text { text: status; wrap: word-wrap; }
            Button {
                text: "Flash";
                primary: true;
                enabled: !busy && drives.length > 0;
                clicked => { flash(drive-index); }
            }
        }
    }
}

/// (target path passed to the CLI, label shown in the picker)
fn targets() -> Vec<(PathBuf, String)> {
    #[cfg(target_os = "linux")]
    {
        let mut v = Vec::new();
        for e in std::fs::read_dir("/sys/block").into_iter().flatten().flatten() {
            let p = e.path();
            let read = |f: &str| std::fs::read_to_string(p.join(f)).unwrap_or_default().trim().to_string();
            let bytes = read("size").parse::<u64>().unwrap_or(0) * 512;
            if read("removable") != "1" || bytes == 0 {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let label = format!("{name} · {} {} · {:.1} GB",
                read("device/vendor"), read("device/model"), bytes as f64 / 1e9);
            v.push((PathBuf::from("/dev").join(&name), label));
        }
        v.sort();
        v
    }
    #[cfg(not(target_os = "linux"))]
    {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
        let p = PathBuf::from(home).join("Downloads").join("thallium-81-amd64.iso");
        vec![(p, "Save the ISO to your Downloads folder (USB writing is Linux-only for now)".into())]
    }
}

fn cli_command(target: &PathBuf, setup: Option<&PathBuf>) -> Command {
    let cli = std::env::current_exe().ok()
        .and_then(|e| e.parent().map(|d| d.join(format!("thallium-flash{}", std::env::consts::EXE_SUFFIX))))
        .unwrap_or_else(|| "thallium-flash".into());
    #[cfg(target_os = "linux")]
    let mut c = if unsafe { libc::geteuid() } == 0 {
        Command::new(&cli)
    } else {
        let mut c = Command::new("pkexec");
        c.arg(&cli);
        c
    };
    #[cfg(not(target_os = "linux"))]
    let mut c = Command::new(&cli);
    c.args(["flash", "--yes", "--machine"]);
    if let Some(s) = setup {
        c.arg("--setup").arg(s);
    }
    c.arg(target);
    c
}

fn opt(s: slint::SharedString) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Build the config from the form, hashing secrets here so plain text never
/// leaves this process. Returns None when nothing was filled in.
fn build_config(a: &App) -> Result<Option<SetupConfig>, String> {
    use sha_crypt::{PasswordHasher, ShaCrypt};
    let account = match opt(a.get_username()) {
        None => None,
        Some(username) => {
            let pw = a.get_password();
            if pw.is_empty() {
                return Err("Set a password for the account, or clear the username.".into());
            }
            let hash = ShaCrypt::SHA512.hash_password(pw.as_bytes()).map_err(|e| format!("hashing password: {e}"))?;
            Some(Account { username, full_name: opt(a.get_full_name()), password_hash: hash.to_string(), autologin: a.get_autologin() })
        }
    };
    let wifi = match opt(a.get_wifi_ssid()) {
        None => None,
        Some(ssid) => {
            let pass = a.get_wifi_pass();
            if !(8..=63).contains(&pass.len()) {
                return Err("Wi-Fi password must be 8–63 characters.".into());
            }
            let mut psk = [0u8; 32];
            pbkdf2::pbkdf2_hmac::<sha1::Sha1>(pass.as_bytes(), ssid.as_bytes(), 4096, &mut psk);
            Some(Wifi { ssid, psk: psk.iter().map(|b| format!("{b:02x}")).collect() })
        }
    };
    let system = System { hostname: opt(a.get_hostname()), timezone: opt(a.get_timezone()), locale: opt(a.get_locale()), keyboard: opt(a.get_keyboard()) };
    let system = (system != System::default()).then_some(system);
    let thallium = Thallium {
        channel: Some(if a.get_channel() == 1 { Channel::Edgerunners } else { Channel::Vanguard }),
        theme: Some(if a.get_theme() == 1 { Theme::Light } else { Theme::Dark }),
        apps: a.get_apps().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        skip_setup: a.get_skip_setup(),
    };
    let cfg = SetupConfig { version: 1, account, system, wifi, thallium: Some(thallium), disk: None };
    cfg.validate().map_err(|e| e.to_string())?;
    Ok(Some(cfg))
}

fn host_timezone() -> String {
    std::fs::read_link("/etc/localtime").ok()
        .and_then(|p| p.to_string_lossy().split("zoneinfo/").nth(1).map(str::to_string))
        .unwrap_or_default()
}

fn gb(b: f64) -> String {
    format!("{:.2} GB", b / 1e9)
}

fn main() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    app.set_can_customize(cfg!(target_os = "linux"));
    app.set_timezone(host_timezone().into());
    app.set_locale(std::env::var("LANG").unwrap_or_default().into());
    let list: Rc<std::cell::RefCell<Vec<(PathBuf, String)>>> = Default::default();

    let load = {
        let (app, list) = (app.as_weak(), list.clone());
        move || {
            let t = targets();
            let labels: Vec<slint::SharedString> = t.iter().map(|(_, l)| l.into()).collect();
            *list.borrow_mut() = t;
            if let Some(app) = app.upgrade() {
                if labels.is_empty() {
                    app.set_status("No USB drive found. Plug one in and press Refresh.".into());
                }
                app.set_drive_index(0);
                app.set_drives(Rc::new(slint::VecModel::from(labels)).into());
            }
        }
    };
    load();
    app.on_refresh(load);

    let weak = app.as_weak();
    app.on_flash(move |i| {
        let Some(target) = list.borrow().get(i as usize).map(|(p, _)| p.clone()) else { return };
        let Some(app) = weak.upgrade() else { return };

        let setup_path = if app.get_customize() {
            let cfg = match build_config(&app) {
                Ok(c) => c,
                Err(e) => { app.set_status(e.into()); return; }
            };
            match cfg.map(|c| write_temp_config(&c)).transpose() {
                Ok(p) => p,
                Err(e) => { app.set_status(format!("Could not save setup config: {e}").into()); return; }
            }
        } else {
            None
        };
        app.set_password("".into());
        app.set_wifi_pass("".into());

        app.set_busy(true);
        app.set_progress(0.0);
        app.set_status("Starting…".into());
        let weak = weak.clone();
        std::thread::spawn(move || {
            let ui = |f: Box<dyn FnOnce(App) + Send>| { let _ = weak.upgrade_in_event_loop(f); };
            let child = cli_command(&target, setup_path.as_ref()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
                    if let Some(p) = &setup_path { let _ = std::fs::remove_file(p); }
                    let msg = format!("Could not start thallium-flash: {e}");
                    ui(Box::new(move |a| { a.set_status(msg.into()); a.set_busy(false); }));
                    return;
                }
            };
            let mut err = child.stderr.take().unwrap();
            let err_thread = std::thread::spawn(move || { let mut s = String::new(); let _ = err.read_to_string(&mut s); s });

            for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
                let p: Vec<&str> = line.split(' ').collect();
                let [phase, done, total] = p[..] else { continue };
                let (done, total) = (done.parse::<f64>().unwrap_or(0.0), total.parse::<f64>().unwrap_or(1.0));
                let word = if phase == "verify" { "Checking" } else { "Downloading and writing" };
                let msg = format!("{word}: {} of {}", gb(done), gb(total));
                let frac = (done / total.max(1.0)) as f32;
                ui(Box::new(move |a| { a.set_progress(frac); a.set_status(msg.into()); }));
            }

            let status = child.wait();
            let stderr = err_thread.join().unwrap_or_default();
            if let Some(p) = &setup_path { let _ = std::fs::remove_file(p); }
            let msg = match status {
                Ok(s) if s.success() => "Done. The drive checks out, so you can remove it and boot Thallium.".to_string(),
                Ok(s) if s.code() == Some(126) => "Cancelled.".to_string(),
                _ => format!("Failed: {}", stderr.lines().last().unwrap_or("unknown error")),
            };
            ui(Box::new(move |a| {
                a.set_progress(if msg.starts_with("Done") { 1.0 } else { 0.0 });
                a.set_status(msg.into());
                a.set_busy(false);
            }));
        });
    });

    app.run()
}

/// Private temp file (0600 on Unix) handed to the CLI, removed after the run.
fn write_temp_config(cfg: &SetupConfig) -> anyhow::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("thallium-setup-{}.toml", std::process::id()));
    let text = cfg.to_toml()?;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    std::io::Write::write_all(&mut o.open(&path)?, text.as_bytes())?;
    Ok(path)
}
