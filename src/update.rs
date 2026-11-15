// openCharts — Copyright (C) 2026 uraXium
// SPDX-License-Identifier: AGPL-3.0-only

//! Checks GitHub for a newer release and installs it.
//!
//! * Installed copy (has `uninstall.exe` next to it): downloads `openCharts-Setup.exe`
//!   and runs it silently; the installer replaces the app and starts it again.
//! * Portable copy: downloads `openCharts.exe`, swaps it in place of the running exe
//!   (Windows allows renaming a running exe) and restarts.

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

pub const REPO: &str = "dev-uraXium/opencharts";
pub const RELEASES_URL: &str = "https://github.com/dev-uraXium/opencharts/releases/latest";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    pub exe_url: Option<String>,
    pub setup_url: Option<String>,
}

pub enum Msg {
    UpToDate,
    Available(Release),
    Failed(String),
    Progress(f32),
    Installed,
}

fn parse_ver(s: &str) -> Vec<u32> {
    s.trim_start_matches(['v', 'V'])
        .split(['.', '-'])
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    let (mut a, mut b) = (parse_ver(remote), parse_ver(local));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .try_proxy_from_env(true)
        .user_agent(concat!("openCharts/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn fetch_latest() -> Result<Release, String> {
    let v: serde_json::Value = agent()
        .get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    let find = |name: &str| {
        assets
            .iter()
            .find(|a| a["name"].as_str().map(|n| n.eq_ignore_ascii_case(name)).unwrap_or(false))
            .and_then(|a| a["browser_download_url"].as_str().map(String::from))
    };
    Ok(Release {
        version: v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string(),
        notes: v["body"].as_str().unwrap_or("").to_string(),
        page: v["html_url"].as_str().unwrap_or(RELEASES_URL).to_string(),
        exe_url: find("openCharts.exe").or_else(|| find("OpenPlates.exe")),
        setup_url: find("openCharts-Setup.exe").or_else(|| find("OpenPlates-Setup.exe")),
    })
}

/// Background check. `manual` = also report "up to date" / errors.
pub fn check(ctx: eframe::egui::Context) -> Receiver<Msg> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let msg = match fetch_latest() {
            Ok(r) if is_newer(&r.version, env!("CARGO_PKG_VERSION")) => Msg::Available(r),
            Ok(_) => Msg::UpToDate,
            Err(e) => Msg::Failed(e),
        };
        let _ = tx.send(msg);
        ctx.request_repaint();
    });
    rx
}

fn exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

pub fn is_installed() -> bool {
    exe_path()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("uninstall.exe").exists()))
        .unwrap_or(false)
}

fn download(url: &str, to: &PathBuf, tx: &std::sync::mpsc::Sender<Msg>, ctx: &eframe::egui::Context) -> Result<(), String> {
    let resp = agent().get(url).call().map_err(|e| e.to_string())?;
    let total: usize = resp.header("Content-Length").and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut r = resp.into_reader();
    let mut buf = Vec::with_capacity(total.max(1 << 20));
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = r.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if total > 0 {
            let _ = tx.send(Msg::Progress(buf.len() as f32 / total as f32));
            ctx.request_repaint();
        }
    }
    if buf.len() < 1_000_000 || &buf[..2] != b"MZ" {
        return Err("download looks incomplete".into());
    }
    std::fs::write(to, &buf).map_err(|e| e.to_string())
}

/// Downloads and installs `rel`, then restarts the app. Runs in the background.
pub fn install(rel: Release, ctx: eframe::egui::Context) -> Receiver<Msg> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let res = (|| -> Result<(), String> {
            let exe = exe_path()?;
            if is_installed() {
                if let Some(url) = &rel.setup_url {
                    let setup = std::env::temp_dir().join("openCharts-Setup.exe");
                    download(url, &setup, &tx, &ctx)?;
                    // silent install; the installer restarts openCharts when done
                    std::process::Command::new(&setup)
                        .arg("/S")
                        .arg("/RELAUNCH")
                        .spawn()
                        .map_err(|e| e.to_string())?;
                    return Ok(());
                }
            }
            let url = rel.exe_url.clone().ok_or("this release has no openCharts.exe")?;
            let new = exe.with_extension("new.exe");
            download(&url, &new, &tx, &ctx)?;
            let old = exe.with_extension("old.exe");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&exe, &old).map_err(|e| format!("couldn't replace the app: {e}"))?;
            if let Err(e) = std::fs::rename(&new, &exe) {
                let _ = std::fs::rename(&old, &exe);
                return Err(format!("couldn't replace the app: {e}"));
            }
            std::process::Command::new(&exe).spawn().map_err(|e| e.to_string())?;
            Ok(())
        })();
        let _ = tx.send(match res {
            Ok(()) => Msg::Installed,
            Err(e) => Msg::Failed(e),
        });
        ctx.request_repaint();
    });
    rx
}

/// Removes the previous exe left behind by a portable self-update.
pub fn cleanup() {
    if let Ok(exe) = exe_path() {
        let _ = std::fs::remove_file(exe.with_extension("old.exe"));
        let _ = std::fs::remove_file(exe.with_extension("new.exe"));
    }
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(0x0800_0000)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}
