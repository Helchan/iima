use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ReplicationCatalog {
    pub reference_branch: &'static str,
    pub reference_commit: &'static str,
    pub reference_build: &'static str,
    pub source_modules: Vec<SourceModule>,
    pub native_dependencies: Vec<NativeDependency>,
    pub acceptance_areas: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceModule {
    pub name: &'static str,
    pub reference: &'static str,
    pub target_owner: &'static str,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct NativeDependency {
    pub name: &'static str,
    pub role: &'static str,
    pub target_strategy: &'static str,
}

pub fn catalog() -> ReplicationCatalog {
    ReplicationCatalog {
        reference_branch: "release/1.3.5",
        reference_commit: "45187444",
        reference_build: "141",
        source_modules: vec![
            SourceModule {
                name: "Playback core",
                reference: "iina/PlayerCore.swift",
                target_owner: "src-tauri/src/player.rs",
                status: "implemented: independent per-session state, playlist/track/filter reducers, lifecycle ordering, command routing, recent/history state, and persistent libmpv execution",
            },
            SourceModule {
                name: "mpv bridge",
                reference: "iina/MPVController.swift",
                target_owner: "src-tauri/src/mpv.rs",
                status: "implemented: ordered startup, 34 observed properties, long-lived clients, wakeup/event drain, hooks, render-context ownership, and package symbol/load verification",
            },
            SourceModule {
                name: "Video surface",
                reference: "iina/VideoView.swift, iina/ViewLayer.swift",
                target_owner: "src-tauri/src/native_video.m, src-tauri/src/mpv.rs",
                status: "implemented: native NSOpenGL/libmpv render child, display-link pacing, fullscreen/PIP/Music Mode migration, ICC/HDR/EDR refresh, and multi-session ownership; physical HDR remains an external acceptance boundary",
            },
            SourceModule {
                name: "Main window",
                reference: "iina/MainWindowController.swift, Base.lproj/MainWindowController.xib",
                target_owner: "src/index.html, src/styles.css, src/main.js",
                status: "implemented: reference OSC/sidebar/playlist/Quick Settings artwork and interaction plus AppKit window geometry, PIP overlay, drop, input, and focus bridges",
            },
            SourceModule {
                name: "Preferences",
                reference: "iina/Preference.swift, Pref*ViewController.swift",
                target_owner: "src-tauri/src/preferences.rs",
                status: "implemented: complete 1.3.5 pane/control model, defaults/dependencies, search/token UX, persistent JSON/plist compatibility, and runtime effect routing",
            },
            SourceModule {
                name: "FFmpeg thumbnails",
                reference: "iina/FFmpegController.m, iina/ThumbnailCache.swift",
                target_owner: "src-tauri/src/media.rs, src-tauri/src/playlist_cache.rs",
                status: "implemented: ffprobe metadata, cancellable 101-frame FFmpeg generation, reference partial cadence, v2 disk cache, eviction, and OSC/playlist consumers",
            },
            SourceModule {
                name: "Plugins",
                reference: "iina/JavascriptPlugin*.swift, iina/JavascriptAPI*.swift",
                target_owner: "src-tauri/src/plugins.rs, src/plugin-realm.js, src/main.js",
                status: "implemented locally: transactional install/update, permissions, isolated entry/global realms, event/input/mpv/network/file/UI APIs, WebView grants, and teardown; real third-party samples remain external acceptance",
            },
        ],
        native_dependencies: vec![
            NativeDependency {
                name: "libmpv",
                role: "Playback, filters, subtitles, scripts, screenshots",
                target_strategy: "Rust/native FFI and Tauri window embedding",
            },
            NativeDependency {
                name: "FFmpeg",
                role: "Media probing and thumbnail generation",
                target_strategy: "Packaged IINA FFmpeg dylibs with native command workers for probing and thumbnails",
            },
            NativeDependency {
                name: "WebKit",
                role: "Tauri WebView and plugin UI surfaces",
                target_strategy: "Tauri window/webview plus custom plugin bridge",
            },
            NativeDependency {
                name: "Sparkle",
                role: "Reference updater",
                target_strategy: "Dynamically loaded packaged Sparkle 2.9.4 with reference or project-owned signed HTTPS appcasts",
            },
        ],
        acceptance_areas: vec![
            "launch",
            "main-window",
            "video-rendering",
            "playback",
            "playlist",
            "quick-settings",
            "preferences",
            "menus",
            "shortcuts",
            "plugins",
            "file-associations",
            "url-scheme",
            "cli",
            "browser-extensions",
            "localization",
            "app-bundle",
            "dmg",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::catalog;

    #[test]
    fn runtime_catalog_does_not_report_completed_modules_as_scaffolds() {
        let catalog = catalog();
        assert_eq!(catalog.reference_branch, "release/1.3.5");
        assert_eq!(catalog.reference_commit, "45187444");
        for module in catalog.source_modules {
            let status = module.status.to_ascii_lowercase();
            assert!(!status.contains("pending"), "{} is stale", module.name);
            assert!(!status.contains("scaffolded"), "{} is stale", module.name);
        }
    }
}
