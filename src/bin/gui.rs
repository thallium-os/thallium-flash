//! Thin GUI over the `thallium-flash` CLI. It runs the CLI as a child
//! process (through pkexec on Linux, so only the disk write runs as root)
//! and turns its `--machine` progress lines into a progress bar.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

slint::slint! {
    import { Button, ComboBox, ProgressIndicator } from "std-widgets.slint";

    export component App inherits Window {
        title: "Thallium Flash";
        preferred-width: 480px;
        in property <[string]> drives;
        in-out property <int> drive-index: 0;
        in property <float> progress;
        in property <string> status: "Pick a USB drive. Everything on it will be erased.";
        in property <bool> busy;
        in property <string> action: "Flash";
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
            ProgressIndicator { progress: progress; height: 6px; }
            Text { text: status; wrap: word-wrap; }
            Button {
                text: action;
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

fn cli_command(target: &PathBuf) -> Command {
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
    c.args(["flash", "--yes", "--machine"]).arg(target);
    c
}

fn gb(b: f64) -> String {
    format!("{:.2} GB", b / 1e9)
}

fn main() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
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
        app.set_busy(true);
        app.set_progress(0.0);
        app.set_status("Starting…".into());
        let weak = weak.clone();
        std::thread::spawn(move || {
            let ui = |f: Box<dyn FnOnce(App) + Send>| { let _ = weak.upgrade_in_event_loop(f); };
            let child = cli_command(&target).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
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
