use crate::{app_logging, localization};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

const RELEASE_VERSION: &str = "1.3.5";
const RELEASE_HIGHLIGHTS_LABEL: &str = "release-highlights";
const LOG_VIEWER_LABEL: &str = crate::auxiliary_player_windows::LOG_VIEWER_WINDOW_LABEL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IinaExternalPage {
    Help,
    GitHub,
    Website,
}

impl IinaExternalPage {
    pub const fn url(self) -> &'static str {
        match self {
            Self::Help => "https://github.com/iina/iina/wiki",
            Self::GitHub => "https://github.com/iina/iina",
            Self::Website => "https://iina.io",
        }
    }
}

pub fn show_first_run_release_highlights<R: Runtime>(
    app: &AppHandle<R>,
    data_directory: &Path,
) -> Result<bool, String> {
    fs::create_dir_all(data_directory).map_err(|error| {
        format!(
            "Unable to create application support directory {}: {error}",
            data_directory.display()
        )
    })?;
    let marker = data_directory.join(format!(".firstLaunchAfter{RELEASE_VERSION}"));
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(_) => {
            show_release_highlights_window(app)?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(format!(
            "Unable to create first-run marker {}: {error}",
            marker.display()
        )),
    }
}

#[tauri::command]
pub fn show_release_highlights(app: AppHandle) -> Result<(), String> {
    show_release_highlights_window(&app)
}

pub fn show_release_highlights_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(RELEASE_HIGHLIGHTS_LABEL) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        RELEASE_HIGHLIGHTS_LABEL,
        WebviewUrl::App("guide.html".into()),
    )
    .title(localization::menu_title("Release Highlights"))
    .inner_size(740.0, 588.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(true)
    .center()
    .build()
    .map(|_| ())
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn close_release_highlights(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(RELEASE_HIGHLIGHTS_LABEL) {
        window.close().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_iina_website() -> Result<String, String> {
    open_iina_external_page(IinaExternalPage::Website)
}

pub fn open_iina_external_page(page: IinaExternalPage) -> Result<String, String> {
    let url = page.url();
    Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map_err(|error| format!("Unable to open IINA page {url}: {error}"))?;
    Ok(url.to_string())
}

pub fn show_log_viewer_window<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let directory = ensure_log_directory(app)?;
    if let Some(window) = app.get_webview_window(LOG_VIEWER_LABEL) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(directory.to_string_lossy().into_owned());
    }
    let window =
        WebviewWindowBuilder::new(app, LOG_VIEWER_LABEL, WebviewUrl::App("log.html".into()))
            .title(localization::menu_title("Log Viewer"))
            .inner_size(600.0, 335.0)
            .resizable(true)
            .decorations(true)
            .center()
            .build()
            .map_err(|error| error.to_string())?;
    crate::auxiliary_player_windows::configure_retained_window(&window, "IINALogViewer")?;
    Ok(directory.to_string_lossy().into_owned())
}

pub fn open_log_directory<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let directory = ensure_log_directory(app)?;
    Command::new("/usr/bin/open")
        .arg(&directory)
        .spawn()
        .map_err(|error| format!("Unable to open the log directory: {error}"))?;
    Ok(directory.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_log_snapshot(app: AppHandle) -> Result<app_logging::AppLogSnapshot, String> {
    ensure_log_directory(&app)?;
    app_logging::snapshot()
}

#[tauri::command]
pub fn save_log_records(app: AppHandle, contents: String) -> Result<Option<String>, String> {
    let destination = app
        .dialog()
        .file()
        .set_title("Log")
        .set_file_name("iina.log")
        .blocking_save_file();
    destination
        .map(|destination| {
            destination
                .into_path()
                .map_err(|error| error.to_string())
                .and_then(|destination| atomic_write(&destination, contents.as_bytes()))
                .map(|destination| destination.to_string_lossy().into_owned())
        })
        .transpose()
}

fn ensure_log_directory<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    if app_logging::directory().is_err() {
        let home = app.path().home_dir().map_err(|error| error.to_string())?;
        app_logging::ensure_initialized(&home)?;
    }
    app_logging::ensure_directory()
}

fn atomic_write(destination: &Path, contents: &[u8]) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", destination.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create {}: {error}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{}.iima-{}-{}.tmp",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("iina.log"),
        std::process::id(),
        SystemTimeNonce::next()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Unable to create {}: {error}", temporary.display()))?;
        file.write_all(contents)
            .map_err(|error| format!("Unable to write {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("Unable to sync {}: {error}", temporary.display()))?;
        fs::rename(&temporary, destination).map_err(|error| {
            format!(
                "Unable to replace {} with {}: {error}",
                destination.display(),
                temporary.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|_| destination.to_path_buf())
}

struct SystemTimeNonce;

impl SystemTimeNonce {
    fn next() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_marker_and_window_contract_match_iina_135() {
        assert_eq!(RELEASE_VERSION, "1.3.5");
        assert_eq!(RELEASE_HIGHLIGHTS_LABEL, "release-highlights");
        let source = include_str!("../../src/guide.html");
        let style = include_str!("../../src/guide.css");
        let runtime = include_str!("../../src/guide.js");
        assert!(source.contains("https://iina.io/highlights/1.3.5/"));
        assert!(source.contains("guide-continue"));
        assert!(style.contains("grid-template-rows: minmax(0, 540px) 48px"));
        assert!(runtime.contains("close_release_highlights"));
        assert!(runtime.contains("open_iina_website"));
    }

    #[test]
    fn built_in_log_viewer_matches_reference_controls_and_geometry() {
        let html = include_str!("../../src/log.html");
        let css = include_str!("../../src/log.css");
        let runtime = include_str!("../../src/log.js");
        for contract in ["Level:", "Subsystem:", "Save as…", "Time", "Message"] {
            assert!(html.contains(contract));
        }
        assert!(css.contains("grid-template-columns: 27px 107px"));
        assert!(runtime.contains("window.setInterval(refresh, 100)"));
        assert!(runtime.contains("navigator.clipboard.writeText"));
        assert!(runtime.contains("save_log_records"));
    }
}
