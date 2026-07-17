use crate::{localization, mpv};
use serde::Serialize;
use std::process::Command;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub(crate) const ABOUT_WINDOW_LABEL: &str = "about-iina";
pub(crate) const IINA_VERSION: &str = "0.9.0";
pub(crate) const IINA_BUILD: &str = "90";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AboutRuntime {
    version: &'static str,
    build: &'static str,
    mpv_version: Option<String>,
    ffmpeg_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AboutLink {
    GitHub,
    Website,
    Email,
    Collider,
    LegacyContributors,
    Gpl,
    Contributors,
    Translators,
}

impl AboutLink {
    fn from_id(value: &str) -> Option<Self> {
        match value {
            "github" => Some(Self::GitHub),
            "website" => Some(Self::Website),
            "email" => Some(Self::Email),
            "collider" => Some(Self::Collider),
            "legacy-contributors" => Some(Self::LegacyContributors),
            "gpl" => Some(Self::Gpl),
            "contributors" => Some(Self::Contributors),
            "translators" => Some(Self::Translators),
            _ => None,
        }
    }

    const fn url(self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/iina/iina",
            Self::Website => "https://iina.io",
            Self::Email => "mailto:developers@iina.io",
            Self::Collider => "https://github.com/lhc70000",
            Self::LegacyContributors => "https://github.com/lhc70000/iina/graphs/contributors",
            Self::Gpl => "https://www.gnu.org/licenses/",
            Self::Contributors => "https://github.com/iina/iina/graphs/contributors",
            Self::Translators => "https://crowdin.com/project/iina/members",
        }
    }
}

fn concise_tool_version(value: Option<String>, tool: &str) -> Option<String> {
    let value = value?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .to_string();
    if value.is_empty() {
        return None;
    }
    if tool == "mpv" {
        let normalized = value
            .strip_prefix("mpv v")
            .or_else(|| value.strip_prefix("mpv "))
            .unwrap_or(&value)
            .split_whitespace()
            .next()?;
        return Some(format!("mpv {normalized}"));
    }
    let normalized = value
        .strip_prefix("ffmpeg version ")
        .or_else(|| value.strip_prefix("FFmpeg "))
        .unwrap_or(&value)
        .split_whitespace()
        .next()
        .unwrap_or(value.as_str());
    Some(format!("FFmpeg {normalized}"))
}

pub fn show_about_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(ABOUT_WINDOW_LABEL) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(
        app,
        ABOUT_WINDOW_LABEL,
        WebviewUrl::App("about.html".into()),
    )
    .title(localization::menu_title_key(
        "AboutWindowController",
        "F0z-JX-Cv5.title",
        "About",
    ))
    .inner_size(640.0, 400.0)
    .min_inner_size(640.0, 400.0)
    .max_inner_size(640.0, 400.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(true)
    .decorations(true);
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    builder
        .center()
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn show_about(app: AppHandle) -> Result<(), String> {
    show_about_window(&app)
}

#[tauri::command]
pub fn get_about_runtime() -> AboutRuntime {
    let (mpv_version, ffmpeg_version) = mpv::libmpv_runtime_versions();
    AboutRuntime {
        version: IINA_VERSION,
        build: IINA_BUILD,
        mpv_version: concise_tool_version(mpv_version, "mpv"),
        ffmpeg_version: concise_tool_version(ffmpeg_version, "ffmpeg"),
    }
}

#[tauri::command]
pub fn open_about_link(link: String) -> Result<String, String> {
    let link = AboutLink::from_id(&link).ok_or_else(|| "Unsupported About link".to_string())?;
    let url = link.url();
    Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map_err(|error| format!("Unable to open About link {url}: {error}"))?;
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_window_matches_project_geometry_identity_and_runtime_copy() {
        let source = include_str!("about_window.rs");
        assert_eq!(ABOUT_WINDOW_LABEL, "about-iina");
        assert_eq!(IINA_VERSION, "0.9.0");
        assert_eq!(IINA_BUILD, "90");
        assert!(source.contains(".inner_size(640.0, 400.0)"));
        assert!(source.contains(".title_bar_style(tauri::TitleBarStyle::Overlay)"));
        assert!(source.contains(".hidden_title(true)"));
        assert!(source.contains("mpv::libmpv_runtime_versions()"));
        assert_eq!(
            concise_tool_version(
                Some("mpv v0.40.0 Copyright mpv/MPlayer/mplayer2 projects".into()),
                "mpv"
            ),
            Some("mpv 0.40.0".into())
        );
        assert_eq!(
            concise_tool_version(Some("ffmpeg version 8.0 Copyright".into()), "ffmpeg"),
            Some("FFmpeg 8.0".into())
        );
    }

    #[test]
    fn about_external_links_are_an_exact_allowlist() {
        assert_eq!(
            AboutLink::from_id("github").map(AboutLink::url),
            Some("https://github.com/iina/iina")
        );
        assert_eq!(
            AboutLink::from_id("website").map(AboutLink::url),
            Some("https://iina.io")
        );
        assert_eq!(
            AboutLink::from_id("email").map(AboutLink::url),
            Some("mailto:developers@iina.io")
        );
        assert_eq!(
            AboutLink::from_id("collider").map(AboutLink::url),
            Some("https://github.com/lhc70000")
        );
        assert_eq!(
            AboutLink::from_id("legacy-contributors").map(AboutLink::url),
            Some("https://github.com/lhc70000/iina/graphs/contributors")
        );
        assert_eq!(
            AboutLink::from_id("gpl").map(AboutLink::url),
            Some("https://www.gnu.org/licenses/")
        );
        assert_eq!(
            AboutLink::from_id("contributors").map(AboutLink::url),
            Some("https://github.com/iina/iina/graphs/contributors")
        );
        assert_eq!(
            AboutLink::from_id("translators").map(AboutLink::url),
            Some("https://crowdin.com/project/iina/members")
        );
        assert!(AboutLink::from_id("https://example.com").is_none());
    }

    #[test]
    fn about_frontend_keeps_reference_tabs_avatar_grid_and_documents() {
        let html = include_str!("../../src/about.html");
        let style = include_str!("../../src/about.css");
        let runtime = include_str!("../../src/about.js");
        for tab in ["license", "contributors", "credits"] {
            assert!(html.contains(&format!("data-tab=\"{tab}\"")));
        }
        assert!(style.contains("grid-template-columns: repeat(auto-fill, 32px)"));
        assert!(runtime.contains("https://api.github.com/repos/iina/iina/contributors"));
        assert!(runtime.contains("about-documents.json"));
        assert!(runtime.contains("open_about_link"));
        assert!(runtime.contains("nextContributorPage"));
        assert!(runtime.contains("safeAvatarUrl"));
    }

    #[test]
    fn about_documents_keep_every_reference_contribution_and_the_full_credits_source() {
        let documents: serde_json::Value =
            serde_json::from_str(include_str!("../../src/assets/iina/about-documents.json"))
                .expect("generated About documents");
        let licenses = documents["licenses"]
            .as_object()
            .expect("license documents");
        let sources = documents["sources"].as_object().expect("document hashes");
        assert_eq!(licenses.len(), 29);
        assert_eq!(
            documents["licenseHtml"]
                .as_object()
                .map(|items| items.len()),
            Some(29)
        );
        assert_eq!(sources.len(), 30);
        assert!(licenses.contains_key("Base"));
        assert!(licenses.contains_key("en"));
        assert!(licenses.contains_key("zh-Hans"));
        assert!(licenses.contains_key("zh-Hant"));
        assert!(documents["credits"]
            .as_str()
            .is_some_and(|credits| credits.len() > 6_000 && credits.contains("libmpv")));
        assert!(documents["creditsHtml"]["body"]
            .as_str()
            .is_some_and(|credits| credits.contains("<b>libmpv</b>")));
    }

    #[test]
    fn app_menu_and_lifecycle_use_the_custom_reusable_about_window() {
        let menu = include_str!("menu.rs");
        let app = include_str!("lib.rs");
        assert!(menu.contains("\"iina.about\""));
        assert!(menu.contains("crate::about_window::show_about_window(app)"));
        assert!(!menu.contains("PredefinedMenuItem::about("));
        for command in ["show_about", "get_about_runtime", "open_about_link"] {
            assert!(app.contains(command), "missing About command {command}");
        }
        assert!(app.contains("label == about_window::ABOUT_WINDOW_LABEL"));
        assert!(app.contains("api.prevent_close()"));
        assert!(app.contains("window.hide()"));
    }
}
