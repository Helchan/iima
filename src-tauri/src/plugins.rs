use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, Runtime, Url};
use tauri_plugin_dialog::DialogExt;

const PLUGINS_DIRECTORY: &str = "plugins";
const PLUGIN_STATE_FILE: &str = "plugins.json";
const PLUGIN_DATA_DIRECTORY: &str = ".data";
const PLUGIN_TEMP_DIRECTORY: &str = ".tmp";
const PLUGIN_PACKAGE_EXTENSION: &str = "iinaplgz";
const PLUGIN_DIRECTORY_EXTENSION: &str = "iinaplugin";
const PLUGIN_DEVELOPMENT_EXTENSION: &str = "iinaplugin-dev";
const MAX_PLUGIN_SCRIPT_COUNT: usize = 200;
const MAX_PLUGIN_SCRIPT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PLUGIN_PAGE_BYTES: u64 = 512 * 1024;
const GITHUB_USER_AGENT: &str = "IINA/1.3.5 (Tauri)";
const MAX_PENDING_PLUGIN_REINSTALLS: usize = 32;
const PLUGIN_REINSTALL_TOKEN_LIFETIME: Duration = Duration::from_secs(15 * 60);
const MAX_PENDING_PLUGIN_PERMISSION_INSTALLS: usize = 32;
const PLUGIN_PERMISSION_TOKEN_LIFETIME: Duration = Duration::from_secs(15 * 60);
const PLUGIN_REPLACEMENT_RECOVERY_PREFIX: &str = ".plugin-recovery-";
const PLUGIN_REPLACEMENT_JOURNAL: &str = "transaction.json";
const PLUGIN_REPLACEMENT_BACKUP: &str = "previous";
static PLUGIN_INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static PENDING_PLUGIN_REINSTALLS: OnceLock<Mutex<BTreeMap<String, PreparedPluginReinstall>>> =
    OnceLock::new();
static PENDING_PLUGIN_PERMISSION_INSTALLS: OnceLock<
    Mutex<BTreeMap<String, PreparedPluginPermissionInstall>>,
> = OnceLock::new();
static PENDING_PLUGIN_INSTALL_NOTIFICATIONS: OnceLock<Mutex<VecDeque<PluginInstallNotification>>> =
    OnceLock::new();
static PLUGIN_STAGING_CLEANUP: OnceLock<Result<(), String>> = OnceLock::new();
static PLUGIN_FILESYSTEM_TRANSACTIONS: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct PluginRecord {
    pub name: String,
    pub identifier: String,
    pub version: String,
    pub description: Option<String>,
    pub author_name: String,
    pub author_url: Option<String>,
    pub author_email: Option<String>,
    pub enabled: bool,
    pub is_external: bool,
    pub permissions: Vec<PluginPermission>,
    pub allowed_domains: Vec<String>,
    pub subtitle_providers: Vec<PluginSubtitleProvider>,
    pub sidebar_tab_name: Option<String>,
    pub github_repo: Option<String>,
    pub github_version: Option<i64>,
    pub preferences_page: Option<String>,
    pub help_page: Option<String>,
    pub preference_defaults: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum PluginInstallResult {
    Installed {
        record: PluginRecord,
    },
    ReinstallConfirmation {
        confirmation: PluginReinstallConfirmation,
    },
    PermissionConfirmation {
        confirmation: PluginPermissionConfirmation,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginReinstallConfirmation {
    pub token: String,
    pub plugin: PluginRecord,
    pub previous_version: String,
    pub existing_is_external: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginPermissionConfirmation {
    pub token: String,
    pub plugin: PluginRecord,
    pub permissions: Vec<PluginPermission>,
    pub only_added: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginInstallNotification {
    pub result: Option<PluginInstallResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginPageContents {
    pub preference_html: Option<String>,
    pub help_html: Option<String>,
    pub help_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PluginFilePath {
    pub path: PathBuf,
    pub is_private: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginWebviewAccess {
    pub root: PathBuf,
    pub plugin_name: String,
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginGithubUpdate {
    pub version: String,
    pub github_version: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginRuntimeSpec {
    pub identifier: String,
    pub name: String,
    pub entry: String,
    pub global_entry: Option<String>,
    pub scripts: BTreeMap<String, String>,
    pub permissions: Vec<String>,
    pub allowed_domains: Vec<String>,
    pub subtitle_providers: Vec<PluginSubtitleProvider>,
    pub sidebar_tab_name: Option<String>,
    pub preference_defaults: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PluginMenuDefinition {
    pub order_index: usize,
    pub owner_label: String,
    pub role: String,
    pub identifier: String,
    pub name: String,
    pub has_global_instance: bool,
    pub items: Vec<PluginMenuItemDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PluginMenuItemDefinition {
    pub id: String,
    pub title: String,
    #[serde(default = "default_menu_item_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub key_binding: Option<String>,
    #[serde(default)]
    pub separator: bool,
    #[serde(default)]
    pub items: Vec<PluginMenuItemDefinition>,
}

fn default_menu_item_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginPermission {
    pub id: String,
    pub dangerous: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginSubtitleProvider {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PluginManifest {
    name: String,
    author: PluginAuthor,
    identifier: String,
    version: String,
    entry: String,
    description: Option<String>,
    #[serde(rename = "globalEntry")]
    global_entry: Option<String>,
    #[serde(rename = "preferencesPage")]
    preferences_page: Option<String>,
    #[serde(rename = "helpPage")]
    help_page: Option<String>,
    permissions: Option<Vec<String>>,
    #[serde(rename = "allowedDomains")]
    allowed_domains: Option<Vec<String>>,
    #[serde(rename = "subtitleProviders")]
    subtitle_providers: Option<Vec<PluginSubtitleProvider>>,
    #[serde(rename = "sidebarTab")]
    sidebar_tab: Option<PluginSidebarTab>,
    #[serde(rename = "preferenceDefaults")]
    preference_defaults: Option<BTreeMap<String, Value>>,
    #[serde(rename = "ghRepo")]
    github_repo: Option<String>,
    #[serde(rename = "ghVersion")]
    github_version: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct PluginAuthor {
    name: String,
    url: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PluginSidebarTab {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubManifestVersion {
    version: String,
    #[serde(rename = "ghVersion")]
    github_version: i64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct PluginState {
    enabled: BTreeMap<String, bool>,
    order: Vec<String>,
}

#[derive(Debug, Clone)]
struct PluginRoot {
    root: PathBuf,
    container: PathBuf,
    is_external: bool,
}

#[derive(Debug)]
struct StagedPluginPackage {
    staging: PathBuf,
    source_root: PathBuf,
    manifest: PluginManifest,
    manifest_bytes: Vec<u8>,
    tree_digest: [u8; 32],
}

impl Drop for StagedPluginPackage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.staging);
    }
}

#[derive(Debug)]
struct ExistingPluginSnapshot {
    container: PathBuf,
    resolved_root: PathBuf,
    is_external: bool,
    container_file_identity: Option<(u64, u64)>,
    manifest_bytes: Vec<u8>,
    development_link_target: Option<PathBuf>,
}

#[derive(Debug)]
struct PreparedPluginReinstall {
    root: PathBuf,
    staged: StagedPluginPackage,
    existing: ExistingPluginSnapshot,
    created_at: Instant,
}

#[derive(Debug)]
struct PreparedPluginPermissionInstall {
    root: PathBuf,
    staged: StagedPluginPackage,
    existing: Option<ExistingPluginSnapshot>,
    previous_version: Option<String>,
    replacement_identifier: Option<String>,
    created_at: Instant,
}

#[derive(Debug, Deserialize, Serialize)]
struct PluginReplacementJournal {
    previous_name: String,
    replacement_name: String,
    #[serde(default)]
    replacement_digest: [u8; 32],
}

pub fn list<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<PluginRecord>, String> {
    let root = plugins_directory(app)?;
    let state = load_state(&root)?;
    scan_plugins(&root, &state)
}

pub fn runtime_specs<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<PluginRuntimeSpec>, String> {
    let root = plugins_directory(app)?;
    let state = load_state(&root)?;
    let mut specs = Vec::new();
    for plugin in plugin_roots(&root)? {
        let manifest = match read_manifest(&plugin.root).and_then(|manifest| {
            validate_manifest(&manifest, &plugin.root)?;
            Ok(manifest)
        }) {
            Ok(manifest) => manifest,
            Err(_) => continue,
        };
        if !state
            .enabled
            .get(&manifest.identifier)
            .copied()
            .unwrap_or(false)
        {
            continue;
        }
        specs.push(runtime_spec_from_manifest(
            &plugin.root,
            &manifest,
            plugin.is_external,
        )?);
    }
    specs.sort_by_key(|spec| {
        state
            .order
            .iter()
            .position(|identifier| identifier == &spec.identifier)
            .unwrap_or(usize::MAX)
    });
    Ok(specs)
}

pub fn plugin_is_enabled(app: &AppHandle, identifier: &str) -> Result<bool, String> {
    let root = plugins_directory(app)?;
    let plugin = plugin_root_for_identifier(&root, identifier)?;
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    let state = load_state(&root)?;
    Ok(state.enabled.get(identifier).copied().unwrap_or(false))
}

pub fn require_plugin_permission(
    app: &AppHandle,
    identifier: &str,
    permission: &str,
) -> Result<(), String> {
    let root = plugins_directory(app)?;
    let plugin = plugin_root_for_identifier(&root, identifier)?;
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    let state = load_state(&root)?;
    if !state.enabled.get(identifier).copied().unwrap_or(false) {
        return Err("Plugin is not enabled".to_string());
    }
    if plugin_declares_permission(&manifest, permission) {
        Ok(())
    } else {
        Err(format!(
            "To call this API, the plugin must declare permission \"{permission}\" in its Info.json."
        ))
    }
}

pub(crate) fn plugin_webview_access(
    app: &AppHandle,
    identifier: &str,
    surface: &str,
    page_path: Option<&str>,
) -> Result<PluginWebviewAccess, String> {
    let plugins_root = plugins_directory(app)?;
    let plugin = plugin_root_for_identifier(&plugins_root, identifier)?;
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    if !load_state(&plugins_root)?
        .enabled
        .get(identifier)
        .copied()
        .unwrap_or(false)
    {
        return Err("Plugin is not enabled".to_string());
    }
    match surface {
        "overlay" if !plugin_declares_permission(&manifest, "video-overlay") => {
            return Err(
                "To call this API, the plugin must declare permission \"video-overlay\" in its Info.json."
                    .to_string(),
            );
        }
        "sidebar" if manifest.sidebar_tab.is_none() => {
            return Err("Plugin did not declare a sidebarTab".to_string());
        }
        "overlay" | "sidebar" | "standalone" => {}
        _ => return Err("Unknown plugin WebView surface".to_string()),
    }
    let canonical_root = fs::canonicalize(&plugin.root)
        .map_err(|error| format!("Unable to resolve plugin directory: {error}"))?;
    if let Some(page_path) = page_path {
        validate_plugin_webview_file(&canonical_root, page_path)?;
    }
    let allowed_domains = if plugin_declares_permission(&manifest, "network-request") {
        manifest.allowed_domains.unwrap_or_default()
    } else {
        Vec::new()
    };
    Ok(PluginWebviewAccess {
        root: canonical_root,
        plugin_name: manifest.name,
        allowed_domains,
    })
}

fn validate_plugin_webview_file(root: &Path, raw_path: &str) -> Result<PathBuf, String> {
    validate_local_plugin_path(raw_path, root, "WebView page")?;
    let resolved = fs::canonicalize(root.join(raw_path))
        .map_err(|error| format!("Unable to resolve plugin WebView page: {error}"))?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err("Plugin WebView page must stay inside its package".to_string());
    }
    Ok(resolved)
}

pub fn page_contents(app: &AppHandle, identifier: &str) -> Result<PluginPageContents, String> {
    let root = plugins_directory(app)?;
    let plugin = plugin_root_for_identifier(&root, identifier)?;
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    let preference_html = manifest
        .preferences_page
        .as_deref()
        .map(|path| read_plugin_page(&plugin.root, path, "preferencesPage"))
        .transpose()?;
    let (help_html, help_url) = match manifest.help_page.as_deref() {
        Some(path) if path.starts_with("https://") || path.starts_with("http://") => {
            (None, Some(path.to_string()))
        }
        Some(path) => (
            Some(read_plugin_page(&plugin.root, path, "helpPage")?),
            None,
        ),
        None => (None, None),
    };
    Ok(PluginPageContents {
        preference_html,
        help_html,
        help_url,
    })
}

pub fn resolve_plugin_file_path(
    app: &AppHandle,
    identifier: &str,
    raw_path: &str,
    current_media: Option<&str>,
) -> Result<PluginFilePath, String> {
    let root = plugins_directory(app)?;
    let plugin = plugin_root_for_identifier(&root, identifier)?;
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    let state = load_state(&root)?;
    if !state.enabled.get(identifier).copied().unwrap_or(false) {
        return Err("Plugin is not enabled".to_string());
    }
    if let Some(path) = plugin_private_destination(&root, identifier, raw_path, "@tmp/")? {
        return Ok(PluginFilePath {
            path,
            is_private: true,
        });
    }
    if let Some(path) = plugin_private_destination(&root, identifier, raw_path, "@data/")? {
        return Ok(PluginFilePath {
            path,
            is_private: true,
        });
    }
    if !plugin_declares_permission(&manifest, "file-system") {
        return Err(
            "To call this API, the plugin must declare permission \"file-system\" in its Info.json."
                .to_string(),
        );
    }
    let path = if let Some(relative) = raw_path.strip_prefix("@current/") {
        let current = current_media
            .filter(|value| Path::new(value).is_absolute())
            .ok_or_else(|| "@current is unavailable without a local media file".to_string())?;
        let parent = Path::new(current)
            .parent()
            .ok_or_else(|| "Current media path has no parent directory".to_string())?;
        standardize_plugin_path(&parent.join(relative))
    } else if let Some(relative) = raw_path.strip_prefix("~/") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "Home directory is unavailable".to_string())?;
        home.join(relative)
    } else {
        PathBuf::from(raw_path)
    };
    if !path.is_absolute() {
        return Err(format!("The path should be an absolute path: {raw_path}"));
    }
    Ok(PluginFilePath {
        path,
        is_private: false,
    })
}

fn plugin_private_destination(
    root: &Path,
    identifier: &str,
    destination: &str,
    prefix: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(relative) = destination.strip_prefix(prefix) else {
        return Ok(None);
    };
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        let symbol = prefix.trim_start_matches('@').trim_end_matches('/');
        return Err(format!(
            "The path does not locate inside the @{symbol} directory: \"{destination}\""
        ));
    }
    let directory = if prefix == "@tmp/" {
        PLUGIN_TEMP_DIRECTORY
    } else {
        PLUGIN_DATA_DIRECTORY
    };
    let private_root = root.join(directory).join(identifier);
    fs::create_dir_all(&private_root)
        .map_err(|error| format!("Unable to create plugin private directory: {error}"))?;
    Ok(Some(standardize_plugin_path(&private_root.join(relative))))
}

fn standardize_plugin_path(path: &Path) -> PathBuf {
    let mut standardized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => standardized.push(prefix.as_os_str()),
            Component::RootDir => standardized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = standardized
                    .components()
                    .next_back()
                    .is_some_and(|last| matches!(last, Component::Normal(_)));
                if can_pop {
                    standardized.pop();
                } else if !standardized.has_root() {
                    standardized.push("..");
                }
            }
            Component::Normal(component) => standardized.push(component),
        }
    }
    standardized
}

fn plugin_declares_permission(manifest: &PluginManifest, permission: &str) -> bool {
    manifest
        .permissions
        .as_deref()
        .unwrap_or_default()
        .iter()
        .any(|declared| declared == permission)
}

pub fn validate_plugin_network_urls(
    app: &AppHandle,
    identifier: &str,
    urls: &[String],
) -> Result<(), String> {
    validate_plugin_network_urls_with_permission(app, identifier, urls, true)
}

pub fn validate_plugin_network_urls_without_permission(
    app: &AppHandle,
    identifier: &str,
    urls: &[String],
) -> Result<(), String> {
    validate_plugin_network_urls_with_permission(app, identifier, urls, false)
}

fn validate_plugin_network_urls_with_permission(
    app: &AppHandle,
    identifier: &str,
    urls: &[String],
    permission_required: bool,
) -> Result<(), String> {
    if urls.is_empty() || urls.len() > 20 {
        return Err("A plugin subtitle download must contain between 1 and 20 URLs".to_string());
    }
    let root = plugins_directory(app)?;
    let plugin = plugin_root_for_identifier(&root, identifier)?;
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    if !load_state(&root)?
        .enabled
        .get(identifier)
        .copied()
        .unwrap_or(false)
    {
        return Err("Plugin is not enabled".to_string());
    }
    if permission_required && !plugin_declares_permission(&manifest, "network-request") {
        return Err("Plugin does not have the network-request permission".to_string());
    }
    let allowed_domains = manifest.allowed_domains.as_deref().unwrap_or_default();
    for value in urls {
        let url =
            Url::parse(value).map_err(|_| "Plugin returned an invalid subtitle URL".to_string())?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Plugin subtitle URLs must use unauthenticated HTTP or HTTPS".to_string());
        }
        let host = url
            .host_str()
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| "Plugin subtitle URL has no host".to_string())?;
        if !allowed_domains
            .iter()
            .any(|domain| plugin_domain_matches(domain, &host))
        {
            return Err(format!("Plugin is not allowed to download from {host}"));
        }
    }
    Ok(())
}

fn plugin_domain_matches(rule: &str, host: &str) -> bool {
    let rule = rule.trim().to_ascii_lowercase();
    rule == "*"
        || rule == host
        || rule
            .strip_prefix("*.")
            .is_some_and(|suffix| host.ends_with(&format!(".{suffix}")))
}

pub fn validate_menu_items(items: &[PluginMenuItemDefinition]) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    validate_menu_items_in_tree(items, &mut ids)
}

fn validate_menu_items_in_tree(
    items: &[PluginMenuItemDefinition],
    ids: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    if items.len() > 100 {
        return Err("A plugin may register at most 100 top-level menu items".to_string());
    }
    for item in items {
        if item.separator {
            if !item.items.is_empty() {
                return Err("A plugin menu separator cannot contain child items".to_string());
            }
            continue;
        }
        if item.id.trim().is_empty()
            || item.id.len() > 128
            || !item.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err("Plugin menu item IDs must be nonempty ASCII letters, digits, hyphens, or underscores".to_string());
        }
        if item.title.trim().is_empty() || item.title.len() > 256 {
            return Err("Plugin menu item titles must be between 1 and 256 characters".to_string());
        }
        if !ids.insert(item.id.clone()) {
            return Err("Plugin menu item IDs must be unique within a plugin".to_string());
        }
        validate_menu_items_in_tree(&item.items, ids)?;
    }
    Ok(())
}

pub fn install_from_dialog(app: &AppHandle) -> Result<Option<PluginInstallResult>, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("Install from local package")
        .set_can_create_directories(false)
        .add_filter("IINA Plugin", &[PLUGIN_PACKAGE_EXTENSION])
        .blocking_pick_file();
    let Some(path) = selected else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|error| error.to_string())?;
    install_package(app, &path).map(Some)
}

pub fn install_package(app: &AppHandle, package: &Path) -> Result<PluginInstallResult, String> {
    prepare_package_permission_install_in_root(&plugins_directory(app)?, package, None)
}

pub fn install_from_github(app: &AppHandle, source: &str) -> Result<PluginInstallResult, String> {
    prepare_from_github_permission_install_in_root(&plugins_directory(app)?, source)
}

pub fn confirm_permissions(app: &AppHandle, token: &str) -> Result<PluginInstallResult, String> {
    let expected_root = plugins_directory(app)?;
    confirm_plugin_permissions_in_root(&expected_root, token)
}

pub fn cancel_permissions(app: &AppHandle, token: &str) -> Result<bool, String> {
    let expected_root = plugins_directory(app)?;
    cancel_plugin_permissions_in_root(&expected_root, token)
}

pub fn confirm_reinstall(app: &AppHandle, token: &str) -> Result<PluginRecord, String> {
    let expected_root = plugins_directory(app)?;
    confirm_plugin_reinstall_in_root(&expected_root, token)
}

pub fn cancel_reinstall(app: &AppHandle, token: &str) -> Result<bool, String> {
    let expected_root = plugins_directory(app)?;
    cancel_plugin_reinstall_in_root(&expected_root, token)
}

pub fn enqueue_install_notification(notification: PluginInstallNotification) -> Result<(), String> {
    pending_plugin_install_notifications()
        .lock()
        .map_err(|error| format!("Unable to lock plugin install notifications: {error}"))?
        .push_back(notification);
    Ok(())
}

pub fn claim_install_notification() -> Result<Option<PluginInstallNotification>, String> {
    let mut pending = pending_plugin_install_notifications()
        .lock()
        .map_err(|error| format!("Unable to lock plugin install notifications: {error}"))?;
    Ok(claim_next_install_notification(&mut pending))
}

pub fn has_pending_install_notification() -> Result<bool, String> {
    pending_plugin_install_notifications()
        .lock()
        .map(|pending| !pending.is_empty())
        .map_err(|error| format!("Unable to lock plugin install notifications: {error}"))
}

fn claim_next_install_notification(
    pending: &mut VecDeque<PluginInstallNotification>,
) -> Option<PluginInstallNotification> {
    pending.pop_front()
}

pub fn check_for_github_update(
    app: &AppHandle,
    identifier: &str,
) -> Result<Option<PluginGithubUpdate>, String> {
    check_for_github_update_in_root(&plugins_directory(app)?, identifier)
}

pub fn update_from_github(
    app: &AppHandle,
    identifier: &str,
) -> Result<PluginInstallResult, String> {
    let root = plugins_directory(app)?;
    let plugin = plugin_root_for_identifier(&root, identifier)?;
    if plugin.is_external {
        return Err(
            "Development plugins are updated from their linked source directory".to_string(),
        );
    }
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    let repository = manifest
        .github_repo
        .as_deref()
        .ok_or_else(|| "Plugin does not declare a GitHub repository".to_string())?;
    if manifest.github_version.is_none() {
        return Err("Plugin does not declare a GitHub version".to_string());
    }
    prepare_github_update_in_root(&root, repository, identifier, &manifest)
}

fn prepare_from_github_permission_install_in_root(
    root: &Path,
    source: &str,
) -> Result<PluginInstallResult, String> {
    let repository = standardize_github_source(source)?;
    let temporary = temporary_plugin_file(root, "github", PLUGIN_PACKAGE_EXTENSION);
    let release_result = download_github_release_package(&repository, &temporary).and_then(|_| {
        prepare_package_permission_install_in_root(root, &temporary, Some(&repository))
    });
    let _ = fs::remove_file(&temporary);
    if let Ok(result) = release_result {
        return Ok(result);
    }

    let source_archive = temporary_plugin_file(root, "github-source", PLUGIN_PACKAGE_EXTENSION);
    let result = download_github_source_archive(&repository, &source_archive).and_then(|_| {
        prepare_package_permission_install_in_root(root, &source_archive, Some(&repository))
    });
    let _ = fs::remove_file(&source_archive);
    result
}

fn prepare_package_permission_install_in_root(
    root: &Path,
    package: &Path,
    github_repository: Option<&str>,
) -> Result<PluginInstallResult, String> {
    let staged = stage_plugin_package(root, package, github_repository, None)?;
    let matches = plugin_roots_for_identifier(root, &staged.manifest.identifier)?;
    if matches.len() > 1 {
        return Err(format!(
            "Multiple plugins use the identifier {}",
            staged.manifest.identifier
        ));
    }
    let state = load_state(root)?;
    let existing = matches.into_iter().next();
    let enabled = existing
        .as_ref()
        .and_then(|_| state.enabled.get(&staged.manifest.identifier).copied())
        .unwrap_or(true);
    let plugin = record_from_manifest(&staged.manifest, false, enabled)?;
    let previous_version = existing
        .as_ref()
        .map(|plugin| read_manifest(&plugin.root).map(|manifest| manifest.version))
        .transpose()?;
    let existing = existing
        .as_ref()
        .map(snapshot_existing_plugin)
        .transpose()?;
    let permissions = plugin.permissions.clone();
    let token = next_plugin_permission_token()?;
    store_pending_plugin_permission_install(
        token.clone(),
        PreparedPluginPermissionInstall {
            root: root.to_path_buf(),
            staged,
            existing,
            previous_version,
            replacement_identifier: None,
            created_at: Instant::now(),
        },
    )?;
    Ok(PluginInstallResult::PermissionConfirmation {
        confirmation: PluginPermissionConfirmation {
            token,
            plugin,
            permissions,
            only_added: false,
        },
    })
}

fn prepare_github_update_in_root(
    root: &Path,
    repository: &str,
    identifier: &str,
    previous_manifest: &PluginManifest,
) -> Result<PluginInstallResult, String> {
    let staged = stage_github_plugin_in_root(root, repository, identifier)?;
    let previous_permissions = previous_manifest
        .permissions
        .as_ref()
        .map(|permissions| {
            permissions
                .iter()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let added_permission_ids = staged
        .manifest
        .permissions
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|permission| !previous_permissions.contains(permission.as_str()))
        .collect::<std::collections::BTreeSet<_>>();
    if added_permission_ids.is_empty() {
        return commit_staged_install(root, &staged, Some(identifier))
            .map(|record| PluginInstallResult::Installed { record });
    }

    let existing = plugin_root_for_identifier(root, identifier)?;
    let existing_snapshot = snapshot_existing_plugin(&existing)?;
    let state = load_state(root)?;
    let enabled = state.enabled.get(identifier).copied().unwrap_or(false);
    let plugin = record_from_manifest(&staged.manifest, false, enabled)?;
    let permissions = plugin
        .permissions
        .iter()
        .filter(|permission| added_permission_ids.contains(&permission.id))
        .cloned()
        .collect();
    let token = next_plugin_permission_token()?;
    store_pending_plugin_permission_install(
        token.clone(),
        PreparedPluginPermissionInstall {
            root: root.to_path_buf(),
            staged,
            existing: Some(existing_snapshot),
            previous_version: Some(previous_manifest.version.clone()),
            replacement_identifier: Some(identifier.to_string()),
            created_at: Instant::now(),
        },
    )?;
    Ok(PluginInstallResult::PermissionConfirmation {
        confirmation: PluginPermissionConfirmation {
            token,
            plugin,
            permissions,
            only_added: true,
        },
    })
}

fn stage_github_plugin_in_root(
    root: &Path,
    repository: &str,
    replacement_identifier: &str,
) -> Result<StagedPluginPackage, String> {
    let repository = standardize_github_source(repository)?;
    let temporary = temporary_plugin_file(root, "github-update", PLUGIN_PACKAGE_EXTENSION);
    let release_result = download_github_release_package(&repository, &temporary).and_then(|_| {
        stage_plugin_package(
            root,
            &temporary,
            Some(&repository),
            Some(replacement_identifier),
        )
    });
    let _ = fs::remove_file(&temporary);
    if let Ok(staged) = release_result {
        return Ok(staged);
    }

    let source_archive =
        temporary_plugin_file(root, "github-update-source", PLUGIN_PACKAGE_EXTENSION);
    let result = download_github_source_archive(&repository, &source_archive).and_then(|_| {
        stage_plugin_package(
            root,
            &source_archive,
            Some(&repository),
            Some(replacement_identifier),
        )
    });
    let _ = fs::remove_file(&source_archive);
    result
}

#[cfg(test)]
fn install_package_in_root(
    root: &Path,
    package: &Path,
    github_repository: Option<&str>,
    replacement_identifier: Option<&str>,
) -> Result<PluginRecord, String> {
    let staged = stage_plugin_package(root, package, github_repository, replacement_identifier)?;
    let result = commit_staged_install(root, &staged, replacement_identifier);
    let _ = fs::remove_dir_all(&staged.staging);
    result
}

#[cfg(test)]
fn prepare_package_install_in_root(
    root: &Path,
    package: &Path,
    github_repository: Option<&str>,
) -> Result<PluginInstallResult, String> {
    let staged = stage_plugin_package(root, package, github_repository, None)?;
    let matches = plugin_roots_for_identifier(root, &staged.manifest.identifier)?;
    if matches.len() > 1 {
        let _ = fs::remove_dir_all(&staged.staging);
        return Err(format!(
            "Multiple plugins use the identifier {}",
            staged.manifest.identifier
        ));
    }
    let Some(existing) = matches.into_iter().next() else {
        let result = commit_new_staged_install(root, &staged)
            .map(|record| PluginInstallResult::Installed { record });
        let _ = fs::remove_dir_all(&staged.staging);
        return result;
    };

    let existing_manifest = read_manifest(&existing.root)?;
    validate_manifest(&existing_manifest, &existing.root)?;
    let existing_snapshot = snapshot_existing_plugin(&existing)?;
    let state = load_state(root)?;
    let enabled = *state
        .enabled
        .get(&staged.manifest.identifier)
        .unwrap_or(&false);
    let plugin = record_from_manifest(&staged.manifest, false, enabled)?;
    let token = next_plugin_reinstall_token()?;
    let confirmation = PluginReinstallConfirmation {
        token: token.clone(),
        plugin,
        previous_version: existing_manifest.version,
        existing_is_external: existing.is_external,
    };
    let pending = PreparedPluginReinstall {
        root: root.to_path_buf(),
        staged,
        existing: existing_snapshot,
        created_at: Instant::now(),
    };
    store_pending_plugin_reinstall(token, pending)?;
    Ok(PluginInstallResult::ReinstallConfirmation { confirmation })
}

fn stage_plugin_package(
    root: &Path,
    package: &Path,
    github_repository: Option<&str>,
    replacement_identifier: Option<&str>,
) -> Result<StagedPluginPackage, String> {
    if !package.is_file() {
        return Err(format!(
            "Plugin package does not exist: {}",
            package.display()
        ));
    }
    if !package
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(PLUGIN_PACKAGE_EXTENSION))
    {
        return Err("Plugin package must use the .iinaplgz extension".to_string());
    }
    verify_zip_entries(package)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let sequence = PLUGIN_INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staging = root.join(format!(
        ".install-{stamp}-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let result = (|| {
        let output = Command::new("/usr/bin/unzip")
            .args(["-qq"])
            .arg(package)
            .arg("-d")
            .arg(&staging)
            .output()
            .map_err(|error| format!("Unable to start unzip: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Unable to unpack plugin package: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        reject_symlinks(&staging)?;
        let source_root = find_plugin_root(&staging)?;
        let manifest = read_manifest(&source_root)?;
        validate_manifest(&manifest, &source_root)?;
        if let Some(repository) = github_repository {
            validate_github_manifest(&manifest, repository)?;
        }
        if let Some(identifier) = replacement_identifier {
            if manifest.identifier != identifier {
                return Err(
                    "Updated plugin identifier does not match the installed plugin".to_string(),
                );
            }
        }
        Ok(StagedPluginPackage {
            staging: staging.clone(),
            manifest_bytes: fs::read(source_root.join("Info.json"))
                .map_err(|error| format!("Unable to snapshot staged plugin manifest: {error}"))?,
            tree_digest: plugin_tree_digest(&source_root)?,
            source_root,
            manifest,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn commit_staged_install(
    root: &Path,
    staged: &StagedPluginPackage,
    replacement_identifier: Option<&str>,
) -> Result<PluginRecord, String> {
    validate_staged_plugin(staged)?;
    let _transaction = plugin_filesystem_transaction_lock()?;
    recover_interrupted_plugin_replacements(root)?;
    let matches = plugin_roots_for_identifier(root, &staged.manifest.identifier)?;
    match replacement_identifier {
        Some(identifier) => {
            if matches.len() != 1 || staged.manifest.identifier != identifier {
                return Err(
                    "Updated plugin identifier does not match the installed plugin".to_string(),
                );
            }
            let existing = &matches[0];
            if existing.is_external {
                return Err(
                    "Development plugins are updated from their linked source directory"
                        .to_string(),
                );
            }
            let state = load_state(root)?;
            let enabled = *state.enabled.get(identifier).unwrap_or(&false);
            replace_plugin_entry_locked(
                &staged.source_root,
                &existing.container,
                &existing.container,
                root,
            )?;
            record_from_manifest(&staged.manifest, false, enabled)
        }
        None => {
            if !matches.is_empty() {
                return Err(format!(
                    "Plugin {} is already installed",
                    staged.manifest.identifier
                ));
            }
            commit_new_staged_install_locked(root, staged, false)
        }
    }
}

#[cfg(test)]
fn commit_new_staged_install(
    root: &Path,
    staged: &StagedPluginPackage,
) -> Result<PluginRecord, String> {
    validate_staged_plugin(staged)?;
    let _transaction = plugin_filesystem_transaction_lock()?;
    recover_interrupted_plugin_replacements(root)?;
    if !plugin_roots_for_identifier(root, &staged.manifest.identifier)?.is_empty() {
        return Err(format!(
            "Plugin {} is already installed",
            staged.manifest.identifier
        ));
    }
    commit_new_staged_install_locked(root, staged, false)
}

fn commit_new_staged_install_enabled(
    root: &Path,
    staged: &StagedPluginPackage,
) -> Result<PluginRecord, String> {
    validate_staged_plugin(staged)?;
    let _transaction = plugin_filesystem_transaction_lock()?;
    recover_interrupted_plugin_replacements(root)?;
    if !plugin_roots_for_identifier(root, &staged.manifest.identifier)?.is_empty() {
        return Err(format!(
            "Plugin {} is already installed",
            staged.manifest.identifier
        ));
    }
    commit_new_staged_install_locked(root, staged, true)
}

fn commit_new_staged_install_locked(
    root: &Path,
    staged: &StagedPluginPackage,
    enabled_by_default: bool,
) -> Result<PluginRecord, String> {
    let target = canonical_plugin_target(root, &staged.manifest.identifier);
    if path_entry_exists(&target) {
        return Err(format!(
            "Plugin target is already occupied: {}",
            target.display()
        ));
    }
    let mut state = load_state(root)?;
    state
        .enabled
        .entry(staged.manifest.identifier.clone())
        .or_insert(enabled_by_default);
    state
        .order
        .retain(|identifier| identifier != &staged.manifest.identifier);
    state.order.push(staged.manifest.identifier.clone());
    sync_plugin_tree(&staged.source_root)?;
    let source_parent = staged.source_root.parent().unwrap_or(root);
    sync_directory(source_parent)?;
    fs::rename(&staged.source_root, &target)
        .map_err(|error| format!("Unable to install plugin: {error}"))?;
    if let Err(error) = sync_directory(source_parent).and_then(|_| sync_directory(root)) {
        return Err(rollback_new_plugin_install(
            &target,
            &staged.source_root,
            source_parent,
            root,
            format!("Unable to persist installed plugin: {error}"),
        ));
    }
    if let Err(error) = save_state(root, &state) {
        return Err(rollback_new_plugin_install(
            &target,
            &staged.source_root,
            source_parent,
            root,
            format!("Unable to save plugin state: {error}"),
        ));
    }
    record_from_manifest(
        &staged.manifest,
        false,
        *state
            .enabled
            .get(&staged.manifest.identifier)
            .unwrap_or(&false),
    )
}

fn rollback_new_plugin_install(
    target: &Path,
    source: &Path,
    source_parent: &Path,
    root: &Path,
    reason: String,
) -> String {
    match fs::rename(target, source) {
        Ok(()) => {
            let _ = sync_directory(source_parent);
            let _ = sync_directory(root);
            reason
        }
        Err(rollback_error) => format!(
            "{reason}; rolling back the new plugin also failed: {rollback_error}; installed path: {}",
            target.display()
        ),
    }
}

fn canonical_plugin_target(root: &Path, identifier: &str) -> PathBuf {
    root.join(format!("{identifier}.{PLUGIN_DIRECTORY_EXTENSION}"))
}

fn plugin_roots_for_identifier(root: &Path, identifier: &str) -> Result<Vec<PluginRoot>, String> {
    let mut matches = Vec::new();
    for plugin in plugin_roots(root)? {
        let Ok(manifest) = read_manifest(&plugin.root) else {
            continue;
        };
        if validate_manifest(&manifest, &plugin.root).is_ok() && manifest.identifier == identifier {
            matches.push(plugin);
        }
    }
    Ok(matches)
}

fn snapshot_existing_plugin(plugin: &PluginRoot) -> Result<ExistingPluginSnapshot, String> {
    let metadata = fs::symlink_metadata(&plugin.container).map_err(|error| error.to_string())?;
    let development_link_target = if plugin.is_external {
        if !metadata.file_type().is_symlink() {
            return Err(
                "A development plugin with this identifier is not a symbolic link; unlink it manually before reinstalling"
                    .to_string(),
            );
        }
        Some(
            fs::read_link(&plugin.container)
                .map_err(|error| format!("Unable to read development plugin link: {error}"))?,
        )
    } else {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Installed plugin container is not a directory".to_string());
        }
        None
    };
    Ok(ExistingPluginSnapshot {
        container: plugin.container.clone(),
        resolved_root: plugin.root.clone(),
        is_external: plugin.is_external,
        container_file_identity: file_identity(&metadata),
        manifest_bytes: fs::read(plugin.root.join("Info.json"))
            .map_err(|error| format!("Unable to snapshot installed plugin: {error}"))?,
        development_link_target,
    })
}

fn next_plugin_reinstall_token() -> Result<String, String> {
    let mut random = [0_u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut random))
        .map_err(|error| format!("Unable to create a plugin reinstall confirmation: {error}"))?;
    let encoded = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("plugin-reinstall-{encoded}"))
}

fn next_plugin_permission_token() -> Result<String, String> {
    let mut random = [0_u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut random))
        .map_err(|error| format!("Unable to create a plugin permission confirmation: {error}"))?;
    let encoded = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("plugin-permission-{encoded}"))
}

fn pending_plugin_reinstalls() -> &'static Mutex<BTreeMap<String, PreparedPluginReinstall>> {
    PENDING_PLUGIN_REINSTALLS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn pending_plugin_permission_installs(
) -> &'static Mutex<BTreeMap<String, PreparedPluginPermissionInstall>> {
    PENDING_PLUGIN_PERMISSION_INSTALLS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn pending_plugin_install_notifications() -> &'static Mutex<VecDeque<PluginInstallNotification>> {
    PENDING_PLUGIN_INSTALL_NOTIFICATIONS.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn purge_expired_plugin_reinstalls(pending: &mut BTreeMap<String, PreparedPluginReinstall>) {
    let expired = pending
        .iter()
        .filter_map(|(token, install)| {
            (install.created_at.elapsed() >= PLUGIN_REINSTALL_TOKEN_LIFETIME).then(|| token.clone())
        })
        .collect::<Vec<_>>();
    for token in expired {
        if let Some(install) = pending.remove(&token) {
            let _ = fs::remove_dir_all(&install.staged.staging);
        }
    }
}

fn purge_expired_plugin_permission_installs(
    pending: &mut BTreeMap<String, PreparedPluginPermissionInstall>,
) {
    pending.retain(|_, install| install.created_at.elapsed() < PLUGIN_PERMISSION_TOKEN_LIFETIME);
}

fn store_pending_plugin_permission_install(
    token: String,
    install: PreparedPluginPermissionInstall,
) -> Result<(), String> {
    let mut pending = pending_plugin_permission_installs()
        .lock()
        .map_err(|error| format!("Unable to lock pending plugin permission prompts: {error}"))?;
    purge_expired_plugin_permission_installs(&mut pending);
    if pending.len() >= MAX_PENDING_PLUGIN_PERMISSION_INSTALLS
        || pending.contains_key(&token)
        || pending.values().any(|existing| {
            existing.root == install.root
                && existing.staged.manifest.identifier == install.staged.manifest.identifier
        })
    {
        return Err("Too many plugin permission confirmations are pending".to_string());
    }
    pending.insert(token, install);
    Ok(())
}

fn take_pending_plugin_permission_install(
    expected_root: &Path,
    token: &str,
) -> Result<Option<PreparedPluginPermissionInstall>, String> {
    let mut pending = pending_plugin_permission_installs()
        .lock()
        .map_err(|error| format!("Unable to lock pending plugin permission prompts: {error}"))?;
    purge_expired_plugin_permission_installs(&mut pending);
    let Some(install) = pending.get(token) else {
        return Ok(None);
    };
    if install.root != expected_root {
        return Err(
            "Plugin permission confirmation belongs to another plugin directory".to_string(),
        );
    }
    Ok(pending.remove(token))
}

fn cancel_plugin_permissions_in_root(root: &Path, token: &str) -> Result<bool, String> {
    Ok(take_pending_plugin_permission_install(root, token)?.is_some())
}

fn confirm_plugin_permissions_in_root(
    root: &Path,
    token: &str,
) -> Result<PluginInstallResult, String> {
    let Some(install) = take_pending_plugin_permission_install(root, token)? else {
        return Err("Plugin permission confirmation expired or was already used".to_string());
    };
    validate_staged_plugin(&install.staged)?;

    if let Some(identifier) = install.replacement_identifier.as_deref() {
        let existing = install
            .existing
            .as_ref()
            .ok_or_else(|| "The plugin update lost its installed-plugin snapshot".to_string())?;
        let matches = plugin_roots_for_identifier(root, identifier)?;
        if matches.len() != 1
            || matches[0].container != existing.container
            || matches[0].root != existing.resolved_root
            || matches[0].is_external != existing.is_external
        {
            return Err("Installed plugin changed before permission confirmation".to_string());
        }
        validate_existing_plugin_snapshot(existing, identifier)?;
        return commit_staged_install(root, &install.staged, Some(identifier))
            .map(|record| PluginInstallResult::Installed { record });
    }

    if let Some(existing) = install.existing {
        let identifier = install.staged.manifest.identifier.clone();
        let state = load_state(root)?;
        let enabled = state.enabled.get(&identifier).copied().unwrap_or(false);
        let plugin = record_from_manifest(&install.staged.manifest, false, enabled)?;
        let confirmation_token = next_plugin_reinstall_token()?;
        let confirmation = PluginReinstallConfirmation {
            token: confirmation_token.clone(),
            plugin,
            previous_version: install.previous_version.unwrap_or_default(),
            existing_is_external: existing.is_external,
        };
        store_pending_plugin_reinstall(
            confirmation_token,
            PreparedPluginReinstall {
                root: install.root,
                staged: install.staged,
                existing,
                created_at: Instant::now(),
            },
        )?;
        return Ok(PluginInstallResult::ReinstallConfirmation { confirmation });
    }

    commit_new_staged_install_enabled(root, &install.staged)
        .map(|record| PluginInstallResult::Installed { record })
}

fn store_pending_plugin_reinstall(
    token: String,
    install: PreparedPluginReinstall,
) -> Result<(), String> {
    let mut pending = pending_plugin_reinstalls()
        .lock()
        .map_err(|error| format!("Unable to lock pending plugin installs: {error}"))?;
    purge_expired_plugin_reinstalls(&mut pending);
    if pending.len() >= MAX_PENDING_PLUGIN_REINSTALLS
        || pending.contains_key(&token)
        || pending.values().any(|existing| {
            existing.root == install.root
                && existing.staged.manifest.identifier == install.staged.manifest.identifier
        })
    {
        let _ = fs::remove_dir_all(&install.staged.staging);
        return Err("Too many plugin reinstall confirmations are pending".to_string());
    }
    pending.insert(token, install);
    Ok(())
}

fn take_pending_plugin_reinstall(
    expected_root: &Path,
    token: &str,
) -> Result<Option<PreparedPluginReinstall>, String> {
    let mut pending = pending_plugin_reinstalls()
        .lock()
        .map_err(|error| format!("Unable to lock pending plugin installs: {error}"))?;
    purge_expired_plugin_reinstalls(&mut pending);
    let Some(install) = pending.get(token) else {
        return Ok(None);
    };
    if install.root != expected_root {
        return Err(
            "Plugin reinstall confirmation belongs to another plugin directory".to_string(),
        );
    }
    Ok(pending.remove(token))
}

fn cancel_plugin_reinstall_in_root(root: &Path, token: &str) -> Result<bool, String> {
    let Some(install) = take_pending_plugin_reinstall(root, token)? else {
        return Ok(false);
    };
    let _ = fs::remove_dir_all(&install.staged.staging);
    Ok(true)
}

fn confirm_plugin_reinstall_in_root(root: &Path, token: &str) -> Result<PluginRecord, String> {
    let Some(install) = take_pending_plugin_reinstall(root, token)? else {
        return Err("Plugin reinstall confirmation expired or was already used".to_string());
    };
    let result = (|| {
        let _transaction = plugin_filesystem_transaction_lock()?;
        recover_interrupted_plugin_replacements(root)?;
        validate_staged_plugin(&install.staged)?;
        let matches = plugin_roots_for_identifier(root, &install.staged.manifest.identifier)?;
        if matches.len() != 1
            || matches[0].container != install.existing.container
            || matches[0].root != install.existing.resolved_root
            || matches[0].is_external != install.existing.is_external
        {
            return Err("Installed plugin set changed before reinstall confirmation".to_string());
        }
        validate_existing_plugin_snapshot(&install.existing, &install.staged.manifest.identifier)?;
        let state = load_state(root)?;
        let enabled = *state
            .enabled
            .get(&install.staged.manifest.identifier)
            .unwrap_or(&false);
        if install.existing.is_external {
            let target = canonical_plugin_target(root, &install.staged.manifest.identifier);
            if path_entry_exists(&target) {
                return Err(format!(
                    "Plugin target is already occupied: {}",
                    target.display()
                ));
            }
            replace_plugin_entry_locked(
                &install.staged.source_root,
                &target,
                &install.existing.container,
                root,
            )?;
        } else {
            replace_plugin_entry_locked(
                &install.staged.source_root,
                &install.existing.container,
                &install.existing.container,
                root,
            )?;
        }
        record_from_manifest(&install.staged.manifest, false, enabled)
    })();
    let _ = fs::remove_dir_all(&install.staged.staging);
    result
}

fn validate_staged_plugin(staged: &StagedPluginPackage) -> Result<(), String> {
    if !staged.source_root.starts_with(&staged.staging) || !staged.source_root.is_dir() {
        return Err("Staged plugin changed before installation".to_string());
    }
    reject_symlinks(&staged.source_root)?;
    let manifest_bytes = fs::read(staged.source_root.join("Info.json"))
        .map_err(|_| "Staged plugin changed before installation".to_string())?;
    if manifest_bytes != staged.manifest_bytes {
        return Err("Staged plugin manifest changed before installation".to_string());
    }
    let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| "Staged plugin manifest changed before installation".to_string())?;
    validate_manifest(&manifest, &staged.source_root)?;
    if manifest.identifier != staged.manifest.identifier
        || plugin_tree_digest(&staged.source_root)? != staged.tree_digest
    {
        return Err("Staged plugin changed before installation".to_string());
    }
    Ok(())
}

fn plugin_tree_digest(root: &Path) -> Result<[u8; 32], String> {
    fn collect(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        let mut entries = fs::read_dir(directory)
            .map_err(|error| format!("Unable to inspect staged plugin: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Unable to inspect staged plugin: {error}"))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("Unable to inspect staged plugin: {error}"))?;
            if metadata.file_type().is_symlink() {
                return Err("Plugin contains a symbolic link and cannot be installed".to_string());
            }
            if metadata.is_dir() {
                collect(root, &path, files)?;
            } else if metadata.is_file() {
                files.push(
                    path.strip_prefix(root)
                        .map_err(|error| error.to_string())?
                        .to_path_buf(),
                );
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    for relative in files {
        let relative_bytes = relative.to_string_lossy();
        digest.update((relative_bytes.len() as u64).to_le_bytes());
        digest.update(relative_bytes.as_bytes());
        let path = root.join(&relative);
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("Unable to inspect staged plugin: {error}"))?;
        digest.update(metadata.len().to_le_bytes());
        let mut file = fs::File::open(path)
            .map_err(|error| format!("Unable to inspect staged plugin: {error}"))?;
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|error| format!("Unable to inspect staged plugin: {error}"))?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
    }
    Ok(digest.finalize().into())
}

fn validate_existing_plugin_snapshot(
    snapshot: &ExistingPluginSnapshot,
    expected_identifier: &str,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&snapshot.container)
        .map_err(|_| "Installed plugin changed before reinstall confirmation".to_string())?;
    if file_identity(&metadata) != snapshot.container_file_identity {
        return Err("Installed plugin changed before reinstall confirmation".to_string());
    }
    if snapshot.is_external {
        if !metadata.file_type().is_symlink()
            || fs::read_link(&snapshot.container).ok() != snapshot.development_link_target
            || fs::canonicalize(&snapshot.container).ok().as_deref()
                != Some(snapshot.resolved_root.as_path())
        {
            return Err(
                "Development plugin link changed before reinstall confirmation".to_string(),
            );
        }
    } else if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Installed plugin changed before reinstall confirmation".to_string());
    }
    let manifest_path = snapshot.resolved_root.join("Info.json");
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|_| "Installed plugin changed before reinstall confirmation".to_string())?;
    if manifest_bytes != snapshot.manifest_bytes {
        return Err("Installed plugin changed before reinstall confirmation".to_string());
    }
    let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| "Installed plugin changed before reinstall confirmation".to_string())?;
    if manifest.identifier != expected_identifier {
        return Err(
            "Installed plugin identifier changed before reinstall confirmation".to_string(),
        );
    }
    Ok(())
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;

    Some((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn file_identity(_metadata: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
fn replace_plugin_entry(source: &Path, replacement: &Path, previous: &Path) -> Result<(), String> {
    let root = replacement
        .parent()
        .ok_or_else(|| "Plugin target has no parent directory".to_string())?;
    let _transaction = plugin_filesystem_transaction_lock()?;
    recover_interrupted_plugin_replacements(root)?;
    replace_plugin_entry_locked(source, replacement, previous, root)
}

fn replace_plugin_entry_locked(
    source: &Path,
    replacement: &Path,
    previous: &Path,
    root: &Path,
) -> Result<(), String> {
    if previous.parent() != Some(root) {
        return Err("Plugin replacement paths must share one plugin directory".to_string());
    }
    let previous_name = recovery_plugin_child_name(root, previous, true, "existing plugin")?;
    let replacement_name =
        recovery_plugin_child_name(root, replacement, false, "replacement plugin")?;
    sync_plugin_tree(source)?;
    if let Some(parent) = source.parent() {
        sync_directory(parent)?;
    }
    let replacement_digest = plugin_tree_digest(source)?;
    let recovery = temporary_plugin_file(root, "plugin-recovery", "transaction");
    fs::create_dir(&recovery)
        .map_err(|error| format!("Unable to create plugin recovery transaction: {error}"))?;
    let journal_path = recovery.join(PLUGIN_REPLACEMENT_JOURNAL);
    let journal = PluginReplacementJournal {
        previous_name,
        replacement_name,
        replacement_digest,
    };
    let journal_result = serde_json::to_vec(&journal)
        .map_err(|error| error.to_string())
        .and_then(|raw| fs::write(&journal_path, raw).map_err(|error| error.to_string()))
        .and_then(|_| {
            fs::File::open(&journal_path)
                .and_then(|file| file.sync_all())
                .map_err(|error| error.to_string())
        })
        .and_then(|_| sync_directory(&recovery))
        .and_then(|_| sync_directory(root));
    if let Err(error) = journal_result {
        let _ = fs::remove_dir_all(&recovery);
        return Err(format!(
            "Unable to write plugin recovery transaction: {error}"
        ));
    }

    let backup = recovery.join(PLUGIN_REPLACEMENT_BACKUP);
    if let Err(error) = fs::rename(previous, &backup) {
        let _ = fs::remove_dir_all(&recovery);
        return Err(format!("Unable to stage existing plugin: {error}"));
    }
    if let Err(sync_error) = sync_directory(&recovery).and_then(|_| sync_directory(root)) {
        return match fs::rename(&backup, previous) {
            Ok(()) => match persist_plugin_rollback(root, &recovery) {
                Ok(()) => Err(format!(
                    "Unable to persist the staged plugin replacement: {sync_error}"
                )),
                Err(rollback_sync_error) => Err(format!(
                    "Unable to persist the staged plugin replacement: {sync_error}; the previous plugin was restored but its rollback could not be persisted: {rollback_sync_error}; recovery transaction: {}",
                    recovery.display()
                )),
            },
            Err(rollback_error) => Err(format!(
                "Unable to persist the staged plugin replacement: {sync_error}; restoring the previous plugin also failed: {rollback_error}; recovery transaction: {}",
                recovery.display()
            )),
        };
    }
    match fs::rename(source, replacement) {
        Ok(()) => {
            let source_parent = source.parent().unwrap_or(root);
            if let Err(error) =
                sync_directory(source_parent).and_then(|_| sync_directory(root))
            {
                eprintln!(
                    "iima: unable to sync plugin replacement: {error}; recovery transaction retained at {}",
                    recovery.display()
                );
                return Ok(());
            }
            let _ = fs::remove_dir_all(&recovery);
            let _ = sync_directory(root);
            Ok(())
        }
        Err(install_error) => match fs::rename(&backup, previous) {
            Ok(()) => match persist_plugin_rollback(root, &recovery) {
                Ok(()) => Err(format!(
                    "Unable to install replacement plugin: {install_error}"
                )),
                Err(rollback_sync_error) => Err(format!(
                    "Unable to install replacement plugin: {install_error}; the previous plugin was restored but its rollback could not be persisted: {rollback_sync_error}; recovery transaction: {}",
                    recovery.display()
                )),
            },
            Err(rollback_error) => Err(format!(
                "Unable to install replacement plugin: {install_error}; restoring the previous plugin also failed: {rollback_error}; recovery transaction: {}",
                recovery.display()
            )),
        },
    }
}

fn plugin_filesystem_transaction_lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    PLUGIN_FILESYSTEM_TRANSACTIONS
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|error| format!("Unable to lock plugin filesystem transactions: {error}"))
}

fn persist_plugin_rollback(root: &Path, recovery: &Path) -> Result<(), String> {
    sync_directory(root)?;
    sync_directory(recovery)?;
    fs::remove_dir_all(recovery)
        .map_err(|error| format!("Unable to close plugin rollback recovery: {error}"))?;
    sync_directory(root)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Unable to sync {}: {error}", path.display()))
}

fn sync_plugin_tree(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Unable to inspect staged plugin for sync: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("Staged plugin contains a symbolic link".to_string());
    }
    if metadata.is_file() {
        return fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("Unable to sync staged plugin file: {error}"));
    }
    if !metadata.is_dir() {
        return Err("Staged plugin contains an unsupported file type".to_string());
    }
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        sync_plugin_tree(&entry.path())?;
    }
    sync_directory(path)
}

fn recovery_plugin_child_name(
    root: &Path,
    path: &Path,
    allow_development: bool,
    label: &str,
) -> Result<String, String> {
    if path.parent() != Some(root) {
        return Err(format!("The {label} is outside the plugin directory"));
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("The {label} name cannot be recovered safely"))?;
    let mut components = Path::new(name).components();
    if name.starts_with('.')
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(format!("The {label} name is unsafe"));
    }
    let extension = Path::new(name).extension().and_then(|value| value.to_str());
    let is_internal =
        extension.is_some_and(|value| value.eq_ignore_ascii_case(PLUGIN_DIRECTORY_EXTENSION));
    let is_development = allow_development
        && extension.is_some_and(|value| value.eq_ignore_ascii_case(PLUGIN_DEVELOPMENT_EXTENSION));
    if !is_internal && !is_development {
        return Err(format!("The {label} has an unsupported extension"));
    }
    Ok(name.to_string())
}

pub fn set_enabled(
    app: &AppHandle,
    identifier: &str,
    enabled: bool,
) -> Result<Vec<PluginRecord>, String> {
    set_enabled_in_root(&plugins_directory(app)?, identifier, enabled)
}

fn set_enabled_in_root(
    root: &Path,
    identifier: &str,
    enabled: bool,
) -> Result<Vec<PluginRecord>, String> {
    let plugin = plugin_root_for_identifier(root, identifier)?;
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    let mut state = load_state(&root)?;
    state.enabled.insert(identifier.to_string(), enabled);
    if !state.order.iter().any(|item| item == identifier) {
        state.order.push(identifier.to_string());
    }
    save_state(&root, &state)?;
    scan_plugins(&root, &state)
}

pub fn reorder(
    app: &AppHandle,
    identifier: &str,
    destination_index: usize,
) -> Result<Vec<PluginRecord>, String> {
    reorder_in_root(&plugins_directory(app)?, identifier, destination_index)
}

fn reorder_in_root(
    root: &Path,
    identifier: &str,
    destination_index: usize,
) -> Result<Vec<PluginRecord>, String> {
    let _transaction = plugin_filesystem_transaction_lock()?;
    recover_interrupted_plugin_replacements(root)?;
    plugin_root_for_identifier(root, identifier)?;
    let mut state = load_state(root)?;
    let records = scan_plugins(root, &state)?;
    if destination_index >= records.len() {
        return Err("Plugin destination index is out of range".to_string());
    }
    let mut order = records
        .iter()
        .map(|record| record.identifier.clone())
        .collect::<Vec<_>>();
    let source_index = order
        .iter()
        .position(|candidate| candidate == identifier)
        .ok_or_else(|| format!("Plugin {identifier} is not installed"))?;
    if source_index != destination_index {
        let moved = order.remove(source_index);
        order.insert(destination_index, moved);
    }
    state.order = order;
    save_state(root, &state)?;
    scan_plugins(root, &state)
}

pub fn installed_root(app: &AppHandle, identifier: &str) -> Result<PathBuf, String> {
    let root = plugins_directory(app)?;
    Ok(plugin_root_for_identifier(&root, identifier)?.root)
}

pub fn remove(app: &AppHandle, identifier: &str) -> Result<Vec<PluginRecord>, String> {
    remove_from_root(&plugins_directory(app)?, identifier)
}

fn remove_from_root(root: &Path, identifier: &str) -> Result<Vec<PluginRecord>, String> {
    let _transaction = plugin_filesystem_transaction_lock()?;
    recover_interrupted_plugin_replacements(root)?;
    let plugin = plugin_root_for_identifier(root, identifier)?;
    let is_linked_development_plugin = plugin.is_external
        && fs::symlink_metadata(&plugin.container)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false);
    if is_linked_development_plugin {
        fs::remove_file(&plugin.container)
            .map_err(|error| format!("Unable to unlink development plugin: {error}"))?;
    } else {
        fs::remove_dir_all(&plugin.container)
            .map_err(|error| format!("Unable to remove plugin: {error}"))?;
    }
    let mut state = load_state(&root)?;
    state.enabled.remove(identifier);
    state.order.retain(|item| item != identifier);
    save_state(&root, &state)?;
    scan_plugins(&root, &state)
}

fn plugins_directory<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?
        .join(PLUGINS_DIRECTORY);
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    if let Err(error) = PLUGIN_STAGING_CLEANUP.get_or_init(|| cleanup_orphaned_staging(&root)) {
        return Err(error.clone());
    }
    Ok(root)
}

fn cleanup_orphaned_staging(root: &Path) -> Result<(), String> {
    let _transaction = plugin_filesystem_transaction_lock()?;
    recover_interrupted_plugin_replacements(root)?;
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if name.starts_with(".install-") && metadata.is_dir() && !metadata.file_type().is_symlink()
        {
            fs::remove_dir_all(&path).map_err(|error| {
                format!("Unable to clean an interrupted plugin installation: {error}")
            })?;
        } else if name.starts_with(".github-") && metadata.is_file() {
            fs::remove_file(&path).map_err(|error| {
                format!("Unable to clean an interrupted plugin download: {error}")
            })?;
        }
    }
    Ok(())
}

fn recover_interrupted_plugin_replacements(root: &Path) -> Result<(), String> {
    let recoveries = fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(PLUGIN_REPLACEMENT_RECOVERY_PREFIX)
                .then(|| entry.path())
        })
        .collect::<Vec<_>>();
    for recovery in recoveries {
        let metadata = fs::symlink_metadata(&recovery).map_err(|error| error.to_string())?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            recover_plugin_replacement(root, &recovery)?;
        }
    }
    Ok(())
}

fn recover_plugin_replacement(root: &Path, recovery: &Path) -> Result<(), String> {
    let backup = recovery.join(PLUGIN_REPLACEMENT_BACKUP);
    if !path_entry_exists(&backup) {
        fs::remove_dir_all(recovery)
            .map_err(|error| format!("Unable to close empty plugin recovery: {error}"))?;
        let _ = sync_directory(root);
        return Ok(());
    }
    let raw = fs::read(recovery.join(PLUGIN_REPLACEMENT_JOURNAL)).map_err(|error| {
        format!(
            "Unable to read interrupted plugin recovery {}: {error}",
            recovery.display()
        )
    })?;
    let journal: PluginReplacementJournal = serde_json::from_slice(&raw).map_err(|error| {
        format!(
            "Unable to parse interrupted plugin recovery {}: {error}",
            recovery.display()
        )
    })?;
    let previous = root.join(&journal.previous_name);
    let replacement = root.join(&journal.replacement_name);
    recovery_plugin_child_name(root, &previous, true, "recovery source")?;
    recovery_plugin_child_name(root, &replacement, false, "recovery target")?;
    if path_entry_exists(&replacement) {
        let metadata = fs::symlink_metadata(&replacement).ok();
        let replacement_is_complete = metadata.is_some_and(|metadata| {
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && plugin_tree_digest(&replacement).ok() == Some(journal.replacement_digest)
        });
        if replacement_is_complete {
            fs::remove_dir_all(recovery).map_err(|error| {
                format!("Unable to finish interrupted plugin replacement: {error}")
            })?;
            let _ = sync_directory(root);
            return Ok(());
        }
        let failed_replacement = recovery.join("incomplete-replacement");
        if path_entry_exists(&failed_replacement) {
            return Err(format!(
                "Plugin recovery {} already contains an incomplete replacement",
                recovery.display()
            ));
        }
        fs::rename(&replacement, &failed_replacement).map_err(|error| {
            format!("Unable to quarantine an incomplete plugin replacement: {error}")
        })?;
        sync_directory(root)?;
        sync_directory(recovery)?;
    }
    if path_entry_exists(&previous) {
        if path_entry_exists(&backup) {
            return Err(format!(
                "Plugin recovery {} contains both the previous plugin and its backup",
                recovery.display()
            ));
        }
        fs::remove_dir_all(recovery)
            .map_err(|error| format!("Unable to close plugin recovery: {error}"))?;
        let _ = sync_directory(root);
        return Ok(());
    }
    if !path_entry_exists(&backup) {
        return Err(format!(
            "Plugin recovery {} does not contain the previous plugin",
            recovery.display()
        ));
    }
    fs::rename(&backup, &previous)
        .map_err(|error| format!("Unable to restore interrupted plugin replacement: {error}"))?;
    sync_directory(root)?;
    sync_directory(recovery)?;
    fs::remove_dir_all(recovery)
        .map_err(|error| format!("Unable to close plugin recovery: {error}"))?;
    let _ = sync_directory(root);
    Ok(())
}

fn path_entry_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn scan_plugins(root: &Path, state: &PluginState) -> Result<Vec<PluginRecord>, String> {
    let mut records = Vec::new();
    for plugin in plugin_roots(root)? {
        let manifest = match read_manifest(&plugin.root).and_then(|manifest| {
            validate_manifest(&manifest, &plugin.root)?;
            Ok(manifest)
        }) {
            Ok(manifest) => manifest,
            Err(_) => continue,
        };
        let enabled = *state.enabled.get(&manifest.identifier).unwrap_or(&false);
        records.push(record_from_manifest(
            &manifest,
            plugin.is_external,
            enabled,
        )?);
    }
    records.sort_by_key(|record| {
        state
            .order
            .iter()
            .position(|identifier| identifier == &record.identifier)
            .unwrap_or(usize::MAX)
    });
    Ok(records)
}

fn record_from_manifest(
    manifest: &PluginManifest,
    is_external: bool,
    enabled: bool,
) -> Result<PluginRecord, String> {
    Ok(PluginRecord {
        name: manifest.name.clone(),
        identifier: manifest.identifier.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        author_name: manifest.author.name.clone(),
        author_url: manifest.author.url.clone(),
        author_email: manifest.author.email.clone(),
        enabled,
        is_external,
        permissions: manifest
            .permissions
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|id| PluginPermission {
                dangerous: matches!(id.as_str(), "network-request" | "file-system"),
                id,
            })
            .collect(),
        allowed_domains: manifest.allowed_domains.clone().unwrap_or_default(),
        subtitle_providers: manifest.subtitle_providers.clone().unwrap_or_default(),
        sidebar_tab_name: manifest
            .sidebar_tab
            .as_ref()
            .and_then(|tab| tab.name.clone()),
        github_repo: manifest.github_repo.clone(),
        github_version: manifest.github_version,
        preferences_page: manifest.preferences_page.clone(),
        help_page: manifest.help_page.clone(),
        preference_defaults: manifest.preference_defaults.clone().unwrap_or_default(),
    })
}

fn validate_github_manifest(manifest: &PluginManifest, repository: &str) -> Result<(), String> {
    if manifest.github_repo.as_deref() != Some(repository) || manifest.github_version.is_none() {
        return Err("GitHub plugin must declare matching ghRepo and ghVersion values".to_string());
    }
    Ok(())
}

fn standardize_github_source(source: &str) -> Result<String, String> {
    let source = source.trim();
    if is_valid_github_repository(source) {
        return Ok(source.to_string());
    }
    let url = Url::parse(source)
        .map_err(|_| "Plugin source must be a GitHub repository URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Plugin source must be an HTTPS github.com owner/repository URL".to_string());
    }
    let parts = url
        .path_segments()
        .ok_or_else(|| "Plugin source must name an owner and repository".to_string())?
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let repository = parts.join("/");
    if parts.len() != 2 || !is_valid_github_repository(&repository) {
        return Err("Plugin source must name an owner and repository".to_string());
    }
    Ok(repository)
}

fn is_valid_github_repository(repository: &str) -> bool {
    let parts = repository.split('/').collect::<Vec<_>>();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
        })
}

fn github_url(repository: &str, path: &str) -> String {
    format!("https://github.com/{repository}/{path}")
}

fn temporary_plugin_file(root: &Path, purpose: &str, extension: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let sequence = PLUGIN_INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    root.join(format!(
        ".{purpose}-{stamp}-{}-{sequence}.{extension}",
        std::process::id()
    ))
}

fn download_github_release_package(repository: &str, destination: &Path) -> Result<(), String> {
    let api_url = format!("https://api.github.com/repos/{repository}/releases/latest");
    let response = curl_output(&api_url)?;
    let release: GithubRelease = serde_json::from_slice(&response)
        .map_err(|_| "GitHub did not return a latest release".to_string())?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name.ends_with(".iinaplgz"))
        .ok_or_else(|| "The latest GitHub release has no .iinaplgz asset".to_string())?;
    validate_github_download_url(&asset.browser_download_url)?;
    curl_download(&asset.browser_download_url, destination)
}

fn download_github_source_archive(repository: &str, destination: &Path) -> Result<(), String> {
    curl_download(&github_url(repository, "archive/main.zip"), destination)
}

fn check_for_github_update_in_root(
    root: &Path,
    identifier: &str,
) -> Result<Option<PluginGithubUpdate>, String> {
    let plugin = plugin_root_for_identifier(root, identifier)?;
    if plugin.is_external {
        return Ok(None);
    }
    let manifest = read_manifest(&plugin.root)?;
    validate_manifest(&manifest, &plugin.root)?;
    let Some(repository) = manifest.github_repo.as_deref() else {
        return Ok(None);
    };
    let Some(current_version) = manifest.github_version else {
        return Ok(None);
    };
    if !is_valid_github_repository(repository) {
        return Err("Plugin declares an invalid ghRepo value".to_string());
    }
    let response = curl_output(&format!(
        "https://raw.githubusercontent.com/{repository}/master/Info.json"
    ))?;
    let remote: GithubManifestVersion = serde_json::from_slice(&response)
        .map_err(|_| "GitHub plugin Info.json is invalid".to_string())?;
    if remote.github_version > current_version {
        Ok(Some(PluginGithubUpdate {
            version: remote.version,
            github_version: remote.github_version,
        }))
    } else {
        Ok(None)
    }
}

fn validate_github_download_url(raw: &str) -> Result<(), String> {
    let url = Url::parse(raw).map_err(|_| "GitHub release asset URL is invalid".to_string())?;
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(
            host,
            "github.com"
                | "objects.githubusercontent.com"
                | "github-releases.githubusercontent.com"
        )
    {
        return Err("GitHub release asset URL is not trusted".to_string());
    }
    Ok(())
}

fn curl_output(url: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "30",
            "--user-agent",
            GITHUB_USER_AGENT,
            url,
        ])
        .output()
        .map_err(|error| format!("Unable to start curl: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "Unable to download GitHub plugin: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn curl_download(url: &str, destination: &Path) -> Result<(), String> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "60",
            "--user-agent",
            GITHUB_USER_AGENT,
            "--output",
        ])
        .arg(destination)
        .arg(url)
        .output()
        .map_err(|error| format!("Unable to start curl: {error}"))?;
    if output.status.success()
        && destination.is_file()
        && destination
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
    {
        Ok(())
    } else {
        let _ = fs::remove_file(destination);
        Err(format!(
            "Unable to download GitHub plugin: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn runtime_spec_from_manifest(
    root: &Path,
    manifest: &PluginManifest,
    allow_symlinks: bool,
) -> Result<PluginRuntimeSpec, String> {
    let scripts = collect_plugin_scripts(root, allow_symlinks)?;
    if !scripts.contains_key(&manifest.entry) {
        return Err(format!(
            "Plugin entry was not collected: {}",
            manifest.entry
        ));
    }
    if let Some(global_entry) = &manifest.global_entry {
        if !scripts.contains_key(global_entry) {
            return Err(format!(
                "Plugin globalEntry was not collected: {global_entry}"
            ));
        }
    }
    Ok(PluginRuntimeSpec {
        identifier: manifest.identifier.clone(),
        name: manifest.name.clone(),
        entry: manifest.entry.clone(),
        global_entry: manifest.global_entry.clone(),
        scripts,
        permissions: manifest.permissions.clone().unwrap_or_default(),
        allowed_domains: manifest.allowed_domains.clone().unwrap_or_default(),
        subtitle_providers: manifest.subtitle_providers.clone().unwrap_or_default(),
        sidebar_tab_name: manifest
            .sidebar_tab
            .as_ref()
            .and_then(|tab| tab.name.clone()),
        preference_defaults: manifest.preference_defaults.clone().unwrap_or_default(),
    })
}

fn collect_plugin_scripts(
    root: &Path,
    allow_symlinks: bool,
) -> Result<BTreeMap<String, String>, String> {
    let mut scripts = BTreeMap::new();
    let mut total_bytes = 0_usize;
    collect_plugin_scripts_from(root, root, allow_symlinks, &mut scripts, &mut total_bytes)?;
    Ok(scripts)
}

fn collect_plugin_scripts_from(
    root: &Path,
    directory: &Path,
    allow_symlinks: bool,
    scripts: &mut BTreeMap<String, String>,
    total_bytes: &mut usize,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let link_metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if link_metadata.file_type().is_symlink() && !allow_symlinks {
            return Err("Plugin contains a symbolic link and cannot be executed".to_string());
        }
        let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
        if path.is_dir() {
            collect_plugin_scripts_from(root, &path, allow_symlinks, scripts, total_bytes)?;
            continue;
        }
        if !path.is_file()
            || !path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("js") || extension.eq_ignore_ascii_case("mjs")
                })
        {
            continue;
        }
        if scripts.len() >= MAX_PLUGIN_SCRIPT_COUNT {
            return Err("Plugin has too many JavaScript files".to_string());
        }
        let file_bytes = usize::try_from(metadata.len())
            .map_err(|_| "Plugin JavaScript file is too large".to_string())?;
        *total_bytes = total_bytes.saturating_add(file_bytes);
        if *total_bytes > MAX_PLUGIN_SCRIPT_BYTES {
            return Err("Plugin JavaScript exceeds the 2 MiB runtime limit".to_string());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        scripts.insert(
            relative,
            fs::read_to_string(&path).map_err(|error| {
                format!("Unable to read plugin script {}: {error}", path.display())
            })?,
        );
    }
    Ok(())
}

fn plugin_root_for_identifier(root: &Path, identifier: &str) -> Result<PluginRoot, String> {
    if !is_valid_identifier(identifier) {
        return Err("Invalid plugin identifier".to_string());
    }
    let matches = plugin_roots(root)?
        .into_iter()
        .filter(|plugin| {
            read_manifest(&plugin.root)
                .and_then(|manifest| {
                    validate_manifest(&manifest, &plugin.root)?;
                    Ok(manifest.identifier == identifier)
                })
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [plugin] => Ok(plugin.clone()),
        [] => Err(format!("Plugin {identifier} is not installed")),
        _ => Err(format!("Multiple plugins use the identifier {identifier}")),
    }
}

fn plugin_roots(root: &Path) -> Result<Vec<PluginRoot>, String> {
    let mut plugins = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let container = entry.path();
        let extension = container.extension().and_then(|value| value.to_str());
        let is_external =
            extension.is_some_and(|value| value.eq_ignore_ascii_case(PLUGIN_DEVELOPMENT_EXTENSION));
        let is_internal =
            extension.is_some_and(|value| value.eq_ignore_ascii_case(PLUGIN_DIRECTORY_EXTENSION));
        if !is_external && !is_internal {
            continue;
        }
        let metadata = fs::symlink_metadata(&container).map_err(|error| error.to_string())?;
        if is_internal && (metadata.file_type().is_symlink() || !metadata.is_dir()) {
            continue;
        }
        let plugin_root = if is_external {
            let Ok(plugin_root) = fs::canonicalize(&container) else {
                continue;
            };
            plugin_root
        } else {
            container.clone()
        };
        if !plugin_root.is_dir() {
            continue;
        }
        plugins.push(PluginRoot {
            root: plugin_root,
            container,
            is_external,
        });
    }
    Ok(plugins)
}

fn read_manifest(root: &Path) -> Result<PluginManifest, String> {
    let raw = fs::read_to_string(root.join("Info.json"))
        .map_err(|error| format!("Unable to read plugin Info.json: {error}"))?;
    serde_json::from_str(&raw).map_err(|error| format!("Invalid plugin Info.json: {error}"))
}

fn validate_manifest(manifest: &PluginManifest, root: &Path) -> Result<(), String> {
    if manifest.name.trim().is_empty()
        || manifest.version.trim().is_empty()
        || manifest.author.name.trim().is_empty()
    {
        return Err("Plugin Info.json has an empty required field".to_string());
    }
    if !is_valid_identifier(&manifest.identifier) {
        return Err("Plugin identifier must use reverse-domain notation".to_string());
    }
    validate_local_plugin_path(&manifest.entry, root, "entry")?;
    for (label, path) in [
        ("globalEntry", manifest.global_entry.as_deref()),
        ("preferencesPage", manifest.preferences_page.as_deref()),
    ] {
        if let Some(path) = path {
            validate_local_plugin_path(path, root, label)?;
        }
    }
    if let Some(help_page) = &manifest.help_page {
        let is_remote = help_page.starts_with("https://") || help_page.starts_with("http://");
        if !is_remote {
            validate_local_plugin_path(help_page, root, "helpPage")?;
        }
    }
    Ok(())
}

fn validate_local_plugin_path(raw_path: &str, root: &Path, label: &str) -> Result<(), String> {
    let path = Path::new(raw_path);
    if raw_path.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "Plugin {label} must be a relative path inside its package"
        ));
    }
    let resolved = root.join(path);
    if !resolved.is_file() {
        return Err(format!("Plugin {label} does not exist: {raw_path}"));
    }
    Ok(())
}

fn read_plugin_page(root: &Path, raw_path: &str, label: &str) -> Result<String, String> {
    validate_local_plugin_path(raw_path, root, label)?;
    let path = root.join(raw_path);
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_PLUGIN_PAGE_BYTES {
        return Err(format!("Plugin {label} exceeds the 512 KiB page limit"));
    }
    fs::read_to_string(&path).map_err(|error| format!("Unable to read plugin {label}: {error}"))
}

fn is_valid_identifier(identifier: &str) -> bool {
    let parts = identifier.split('.').collect::<Vec<_>>();
    parts.len() >= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
        })
}

fn load_state(root: &Path) -> Result<PluginState, String> {
    let path = root.join(PLUGIN_STATE_FILE);
    if !path.exists() {
        return Ok(PluginState::default());
    }
    let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw).map_err(|error| format!("Unable to read plugin state: {error}"))
}

fn save_state(root: &Path, state: &PluginState) -> Result<(), String> {
    let path = root.join(PLUGIN_STATE_FILE);
    let temporary = root.join(format!(".{PLUGIN_STATE_FILE}.tmp"));
    let raw = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
    fs::write(&temporary, raw).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

fn verify_zip_entries(package: &Path) -> Result<(), String> {
    let output = Command::new("/usr/bin/unzip")
        .args(["-Z1"])
        .arg(package)
        .output()
        .map_err(|error| format!("Unable to inspect plugin package: {error}"))?;
    if !output.status.success() {
        return Err("Unable to inspect plugin package".to_string());
    }
    for entry in String::from_utf8_lossy(&output.stdout).lines() {
        let path = Path::new(entry);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err("Plugin package contains an unsafe path".to_string());
        }
    }
    Ok(())
}

fn reject_symlinks(root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Plugin package contains a symbolic link".to_string());
        }
        if metadata.is_dir() {
            reject_symlinks(&path)?;
        }
    }
    Ok(())
}

fn find_plugin_root(staging: &Path) -> Result<PathBuf, String> {
    if staging.join("Info.json").is_file() {
        return Ok(staging.to_path_buf());
    }
    let entries = fs::read_dir(staging)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let roots = entries
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("Info.json").is_file())
        .collect::<Vec<_>>();
    if roots.len() == 1 {
        Ok(roots[0].clone())
    } else {
        Err("Plugin package must contain exactly one Info.json root".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_test_root(label: &str) -> PathBuf {
        let sequence = PLUGIN_INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "iima-plugin-{label}-{}-{sequence}",
            std::process::id()
        ))
    }

    fn write_plugin_fixture(directory: &Path, identifier: &str, version: &str, entry: &str) {
        fs::create_dir_all(directory).unwrap();
        fs::write(
            directory.join("Info.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "name": "Reinstall Fixture",
                "author": {"name": "IINA"},
                "identifier": identifier,
                "version": version,
                "entry": "entry.js"
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(directory.join("entry.js"), entry).unwrap();
    }

    fn package_plugin_fixture(
        root: &Path,
        identifier: &str,
        version: &str,
        entry: &str,
    ) -> PathBuf {
        let sequence = PLUGIN_INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let source_name = format!("package-source-{sequence}");
        let source = root.join(&source_name);
        write_plugin_fixture(&source, identifier, version, entry);
        let package = root.join(format!("fixture-{sequence}.iinaplgz"));
        let output = Command::new("/usr/bin/zip")
            .args(["-q", "-r"])
            .arg(&package)
            .arg(&source_name)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(output.status.success());
        package
    }

    fn package_plugin_fixture_with_permissions(
        root: &Path,
        identifier: &str,
        version: &str,
        permissions: &[&str],
    ) -> PathBuf {
        let sequence = PLUGIN_INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let source_name = format!("permission-package-source-{sequence}");
        let source = root.join(&source_name);
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("Info.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "name": "Permission Fixture",
                "author": {"name": "IINA"},
                "identifier": identifier,
                "version": version,
                "entry": "entry.js",
                "permissions": permissions,
                "allowedDomains": ["example.com"]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(source.join("entry.js"), "module.exports = true;").unwrap();
        let package = root.join(format!("permission-fixture-{sequence}.iinaplgz"));
        let output = Command::new("/usr/bin/zip")
            .args(["-q", "-r"])
            .arg(&package)
            .arg(&source_name)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(output.status.success());
        package
    }

    fn permission_confirmation(result: PluginInstallResult) -> PluginPermissionConfirmation {
        match result {
            PluginInstallResult::PermissionConfirmation { confirmation } => confirmation,
            PluginInstallResult::Installed { .. }
            | PluginInstallResult::ReinstallConfirmation { .. } => {
                panic!("expected permission confirmation")
            }
        }
    }

    fn reinstall_confirmation(result: PluginInstallResult) -> PluginReinstallConfirmation {
        match result {
            PluginInstallResult::ReinstallConfirmation { confirmation } => confirmation,
            PluginInstallResult::Installed { .. }
            | PluginInstallResult::PermissionConfirmation { .. } => {
                panic!("expected reinstall confirmation")
            }
        }
    }

    fn staged_source_for_token(token: &str) -> PathBuf {
        pending_plugin_reinstalls()
            .lock()
            .unwrap()
            .get(token)
            .unwrap()
            .staged
            .source_root
            .clone()
    }

    fn assert_no_install_staging(root: &Path) {
        assert!(!fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| { entry.file_name().to_string_lossy().starts_with(".install-") }));
    }

    #[test]
    fn new_install_requires_permission_confirmation_and_cancel_never_mutates_disk() {
        let root = plugin_test_root("permission-cancel");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.permissioncancel";
        let package = package_plugin_fixture_with_permissions(
            &root,
            identifier,
            "1.0.0",
            &["show-osd", "file-system"],
        );

        let confirmation = permission_confirmation(
            prepare_package_permission_install_in_root(&root, &package, None).unwrap(),
        );
        assert!(confirmation.token.starts_with("plugin-permission-"));
        assert_eq!(confirmation.plugin.identifier, identifier);
        assert_eq!(confirmation.permissions.len(), 2);
        assert!(!confirmation.only_added);
        assert!(!canonical_plugin_target(&root, identifier).exists());

        assert!(cancel_plugin_permissions_in_root(&root, &confirmation.token).unwrap());
        assert!(!cancel_plugin_permissions_in_root(&root, &confirmation.token).unwrap());
        assert!(confirm_plugin_permissions_in_root(&root, &confirmation.token).is_err());
        assert!(!canonical_plugin_target(&root, identifier).exists());
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn confirmed_new_plugin_is_committed_enabled_and_token_is_single_use() {
        let root = plugin_test_root("permission-confirm-new");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.permissionconfirm";
        let package = package_plugin_fixture_with_permissions(
            &root,
            identifier,
            "1.0.0",
            &["network-request"],
        );
        let confirmation = permission_confirmation(
            prepare_package_permission_install_in_root(&root, &package, None).unwrap(),
        );

        let result = confirm_plugin_permissions_in_root(&root, &confirmation.token).unwrap();
        let PluginInstallResult::Installed { record } = result else {
            panic!("expected installed plugin");
        };
        assert!(record.enabled);
        assert!(canonical_plugin_target(&root, identifier).is_dir());
        assert_eq!(
            load_state(&root).unwrap().enabled.get(identifier),
            Some(&true)
        );
        assert!(confirm_plugin_permissions_in_root(&root, &confirmation.token).is_err());
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_install_keeps_old_plugin_until_permission_and_reinstall_confirmations() {
        let root = plugin_test_root("permission-then-reinstall");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.permissionreinstall";
        let installed = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&installed, identifier, "1.0.0", "old");
        save_state(
            &root,
            &PluginState {
                enabled: BTreeMap::from([(identifier.to_string(), true)]),
                order: vec![identifier.to_string()],
            },
        )
        .unwrap();
        let package =
            package_plugin_fixture_with_permissions(&root, identifier, "2.0.0", &["file-system"]);
        let permission = permission_confirmation(
            prepare_package_permission_install_in_root(&root, &package, None).unwrap(),
        );

        let reinstall = reinstall_confirmation(
            confirm_plugin_permissions_in_root(&root, &permission.token).unwrap(),
        );
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "old"
        );
        let record = confirm_plugin_reinstall_in_root(&root, &reinstall.token).unwrap();
        assert_eq!(record.version, "2.0.0");
        assert!(record.enabled);
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "module.exports = true;"
        );
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_install_cancel_is_single_use_and_leaves_old_plugin_and_state_unchanged() {
        let root = plugin_test_root("reinstall-cancel");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallcancel";
        let installed = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&installed, identifier, "1.0.0", "module.exports = 'old';");
        let state = PluginState {
            enabled: BTreeMap::from([(identifier.to_string(), true)]),
            order: vec!["before".into(), identifier.into(), "after".into()],
        };
        save_state(&root, &state).unwrap();
        let state_bytes = fs::read(root.join(PLUGIN_STATE_FILE)).unwrap();
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "module.exports = 'new';");

        let confirmation =
            reinstall_confirmation(prepare_package_install_in_root(&root, &package, None).unwrap());
        assert!(confirmation.token.starts_with("plugin-reinstall-"));
        let opaque_token = confirmation
            .token
            .strip_prefix("plugin-reinstall-")
            .unwrap();
        assert_eq!(opaque_token.len(), 32);
        assert!(opaque_token
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
        assert!(!serde_json::to_string(&confirmation)
            .unwrap()
            .contains(&root.display().to_string()));
        assert_eq!(confirmation.plugin.identifier, identifier);
        assert_eq!(confirmation.previous_version, "1.0.0");
        assert!(confirmation.plugin.enabled);
        assert!(!confirmation.existing_is_external);
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "module.exports = 'old';"
        );
        assert_eq!(fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(), state_bytes);

        assert!(cancel_plugin_reinstall_in_root(&root, &confirmation.token).unwrap());
        assert!(!cancel_plugin_reinstall_in_root(&root, &confirmation.token).unwrap());
        assert!(confirm_plugin_reinstall_in_root(&root, &confirmation.token).is_err());
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "module.exports = 'old';"
        );
        assert_eq!(fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(), state_bytes);
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reinstall_confirmation_rejects_the_wrong_plugin_root_without_consuming_the_token() {
        let root = plugin_test_root("reinstall-wrong-root");
        let other_root = plugin_test_root("reinstall-wrong-root-other");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&other_root);
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&other_root).unwrap();
        let identifier = "io.iina.reinstallwrongroot";
        write_plugin_fixture(
            &canonical_plugin_target(&root, identifier),
            identifier,
            "1.0.0",
            "old",
        );
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "new");
        let confirmation =
            reinstall_confirmation(prepare_package_install_in_root(&root, &package, None).unwrap());

        let error = cancel_plugin_reinstall_in_root(&other_root, &confirmation.token).unwrap_err();
        assert!(error.contains("another plugin directory"));
        assert!(cancel_plugin_reinstall_in_root(&root, &confirmation.token).unwrap());
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(other_root);
    }

    #[test]
    fn expired_reinstall_confirmation_cleans_staging_and_cannot_be_used() {
        let root = plugin_test_root("reinstall-expired");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallexpired";
        write_plugin_fixture(
            &canonical_plugin_target(&root, identifier),
            identifier,
            "1.0.0",
            "old",
        );
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "new");
        let confirmation =
            reinstall_confirmation(prepare_package_install_in_root(&root, &package, None).unwrap());
        let staging = pending_plugin_reinstalls()
            .lock()
            .unwrap()
            .get(&confirmation.token)
            .unwrap()
            .staged
            .staging
            .clone();
        pending_plugin_reinstalls()
            .lock()
            .unwrap()
            .get_mut(&confirmation.token)
            .unwrap()
            .created_at = Instant::now() - PLUGIN_REINSTALL_TOKEN_LIFETIME - Duration::from_secs(1);

        assert!(!cancel_plugin_reinstall_in_root(&root, &confirmation.token).unwrap());
        assert!(!staging.exists());
        assert!(confirm_plugin_reinstall_in_root(&root, &confirmation.token).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_install_confirm_atomically_replaces_and_preserves_enabled_and_order() {
        let root = plugin_test_root("reinstall-confirm");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallconfirm";
        let installed = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&installed, identifier, "1.0.0", "module.exports = 'old';");
        let state = PluginState {
            enabled: BTreeMap::from([(identifier.to_string(), true)]),
            order: vec!["before".into(), identifier.into(), "after".into()],
        };
        save_state(&root, &state).unwrap();
        let state_bytes = fs::read(root.join(PLUGIN_STATE_FILE)).unwrap();
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "module.exports = 'new';");
        let confirmation =
            reinstall_confirmation(prepare_package_install_in_root(&root, &package, None).unwrap());

        let record = confirm_plugin_reinstall_in_root(&root, &confirmation.token).unwrap();
        assert_eq!(record.version, "2.0.0");
        assert!(record.enabled);
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "module.exports = 'new';"
        );
        assert_eq!(fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(), state_bytes);
        assert!(confirm_plugin_reinstall_in_root(&root, &confirmation.token).is_err());
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_install_confirm_uses_latest_enabled_state_without_rewriting_order() {
        let root = plugin_test_root("reinstall-latest-state");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstalllateststate";
        let installed = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&installed, identifier, "1.0.0", "old");
        save_state(
            &root,
            &PluginState {
                enabled: BTreeMap::from([(identifier.to_string(), true)]),
                order: vec![identifier.into(), "other".into()],
            },
        )
        .unwrap();
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "new");
        let confirmation =
            reinstall_confirmation(prepare_package_install_in_root(&root, &package, None).unwrap());

        save_state(
            &root,
            &PluginState {
                enabled: BTreeMap::from([(identifier.to_string(), false)]),
                order: vec!["other".into(), identifier.into(), "latest".into()],
            },
        )
        .unwrap();
        let latest_state_bytes = fs::read(root.join(PLUGIN_STATE_FILE)).unwrap();

        let record = confirm_plugin_reinstall_in_root(&root, &confirmation.token).unwrap();
        assert!(!record.enabled);
        assert_eq!(record.version, "2.0.0");
        assert_eq!(
            fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(),
            latest_state_bytes
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_install_prepare_rejects_multiple_existing_plugins_without_mutation() {
        let root = plugin_test_root("reinstall-existing-duplicates");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallexistingduplicates";
        let installed = canonical_plugin_target(&root, identifier);
        let duplicate = root.join("duplicate.iinaplugin");
        write_plugin_fixture(&installed, identifier, "1.0.0", "first");
        write_plugin_fixture(&duplicate, identifier, "1.1.0", "second");
        save_state(
            &root,
            &PluginState {
                enabled: BTreeMap::from([(identifier.to_string(), true)]),
                order: vec![identifier.into()],
            },
        )
        .unwrap();
        let state_bytes = fs::read(root.join(PLUGIN_STATE_FILE)).unwrap();
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "replacement");

        let error = prepare_package_install_in_root(&root, &package, None).unwrap_err();
        assert!(error.contains("Multiple plugins use the identifier"));
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "first"
        );
        assert_eq!(
            fs::read_to_string(duplicate.join("entry.js")).unwrap(),
            "second"
        );
        assert_eq!(fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(), state_bytes);
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_plugin_state_never_commits_a_fresh_install_or_github_update() {
        let fresh_root = plugin_test_root("install-invalid-state");
        let _ = fs::remove_dir_all(&fresh_root);
        fs::create_dir_all(&fresh_root).unwrap();
        fs::write(fresh_root.join(PLUGIN_STATE_FILE), b"{").unwrap();
        let fresh_identifier = "io.iina.installinvalidstate";
        let fresh_package = package_plugin_fixture(&fresh_root, fresh_identifier, "1.0.0", "fresh");
        let fresh_staged = stage_plugin_package(&fresh_root, &fresh_package, None, None).unwrap();

        assert!(commit_new_staged_install(&fresh_root, &fresh_staged).is_err());
        assert!(!canonical_plugin_target(&fresh_root, fresh_identifier).exists());
        assert!(fresh_staged.source_root.is_dir());
        drop(fresh_staged);
        assert_no_install_staging(&fresh_root);

        let update_root = plugin_test_root("update-invalid-state");
        let _ = fs::remove_dir_all(&update_root);
        fs::create_dir_all(&update_root).unwrap();
        let update_identifier = "io.iina.updateinvalidstate";
        let installed = canonical_plugin_target(&update_root, update_identifier);
        write_plugin_fixture(&installed, update_identifier, "1.0.0", "old");
        fs::write(update_root.join(PLUGIN_STATE_FILE), b"{").unwrap();
        let update_package =
            package_plugin_fixture(&update_root, update_identifier, "2.0.0", "new");
        let update_staged =
            stage_plugin_package(&update_root, &update_package, None, Some(update_identifier))
                .unwrap();

        assert!(
            commit_staged_install(&update_root, &update_staged, Some(update_identifier)).is_err()
        );
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "old"
        );
        assert!(update_staged.source_root.is_dir());
        drop(update_staged);
        assert_no_install_staging(&update_root);

        let _ = fs::remove_dir_all(fresh_root);
        let _ = fs::remove_dir_all(update_root);
    }

    #[test]
    fn reinstall_confirmation_rejects_a_duplicate_added_after_prepare_without_mutation() {
        let root = plugin_test_root("reinstall-race");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallrace";
        let installed = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&installed, identifier, "1.0.0", "module.exports = 'old';");
        let state = PluginState {
            enabled: BTreeMap::from([(identifier.to_string(), false)]),
            order: vec![identifier.into()],
        };
        save_state(&root, &state).unwrap();
        let state_bytes = fs::read(root.join(PLUGIN_STATE_FILE)).unwrap();
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "new");
        let confirmation =
            reinstall_confirmation(prepare_package_install_in_root(&root, &package, None).unwrap());
        let duplicate = root.join("duplicate.iinaplugin");
        write_plugin_fixture(&duplicate, identifier, "1.1.0", "duplicate");

        assert!(confirm_plugin_reinstall_in_root(&root, &confirmation.token).is_err());
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "module.exports = 'old';"
        );
        assert_eq!(
            fs::read_to_string(duplicate.join("entry.js")).unwrap(),
            "duplicate"
        );
        assert_eq!(fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(), state_bytes);
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reinstall_confirmation_rejects_tampered_staging_and_consumes_token() {
        let root = plugin_test_root("reinstall-tamper");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstalltamper";
        let installed = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&installed, identifier, "1.0.0", "old");
        let state = PluginState {
            enabled: BTreeMap::from([(identifier.to_string(), true)]),
            order: vec![identifier.into()],
        };
        save_state(&root, &state).unwrap();
        let state_bytes = fs::read(root.join(PLUGIN_STATE_FILE)).unwrap();
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "new");
        let confirmation =
            reinstall_confirmation(prepare_package_install_in_root(&root, &package, None).unwrap());
        let staged_source = staged_source_for_token(&confirmation.token);
        fs::write(staged_source.join("entry.js"), "tampered").unwrap();

        assert!(confirm_plugin_reinstall_in_root(&root, &confirmation.token).is_err());
        assert!(confirm_plugin_reinstall_in_root(&root, &confirmation.token).is_err());
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "old"
        );
        assert_eq!(fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(), state_bytes);
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn packaged_reinstall_safely_unlinks_development_plugin_without_deleting_its_source() {
        use std::os::unix::fs::symlink;

        let root = plugin_test_root("reinstall-development");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstalldevelopment";
        let development_source = root.join("development-source");
        write_plugin_fixture(&development_source, identifier, "1.0.0", "development");
        let development_link = root.join("development.iinaplugin-dev");
        symlink(&development_source, &development_link).unwrap();
        let state = PluginState {
            enabled: BTreeMap::from([(identifier.to_string(), true)]),
            order: vec!["before".into(), identifier.into(), "after".into()],
        };
        save_state(&root, &state).unwrap();
        let state_bytes = fs::read(root.join(PLUGIN_STATE_FILE)).unwrap();
        let package = package_plugin_fixture(&root, identifier, "2.0.0", "packaged");
        let confirmation =
            reinstall_confirmation(prepare_package_install_in_root(&root, &package, None).unwrap());
        assert!(confirmation.existing_is_external);

        let record = confirm_plugin_reinstall_in_root(&root, &confirmation.token).unwrap();
        assert!(!record.is_external);
        assert!(record.enabled);
        assert!(!development_link.exists());
        assert!(development_source.is_dir());
        assert_eq!(
            fs::read_to_string(development_source.join("entry.js")).unwrap(),
            "development"
        );
        assert_eq!(
            fs::read_to_string(canonical_plugin_target(&root, identifier).join("entry.js"))
                .unwrap(),
            "packaged"
        );
        assert_eq!(fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(), state_bytes);
        assert_no_install_staging(&root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plugin_install_notifications_are_claimed_fifo_and_once() {
        let mut pending = VecDeque::from([
            PluginInstallNotification {
                result: None,
                error: Some("first".into()),
            },
            PluginInstallNotification {
                result: None,
                error: Some("second".into()),
            },
        ]);

        assert_eq!(
            claim_next_install_notification(&mut pending)
                .unwrap()
                .error
                .as_deref(),
            Some("first")
        );
        assert_eq!(
            claim_next_install_notification(&mut pending)
                .unwrap()
                .error
                .as_deref(),
            Some("second")
        );
        assert!(claim_next_install_notification(&mut pending).is_none());
    }

    #[test]
    fn hidden_recovery_backups_are_not_scanned_as_plugins_and_orphan_staging_is_cleaned() {
        let root = plugin_test_root("reinstall-recovery-artifacts");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallrecoveryartifacts";
        let installed = canonical_plugin_target(&root, identifier);
        let backup = root.join(".plugin-backup-old.iinaplugin");
        let orphan = root.join(".install-interrupted");
        let download = root.join(".github-interrupted.iinaplgz");
        write_plugin_fixture(&installed, identifier, "1.0.0", "installed");
        write_plugin_fixture(&backup, identifier, "0.9.0", "backup");
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("partial"), "staged").unwrap();
        fs::write(&download, "partial").unwrap();

        let matches = plugin_roots_for_identifier(&root, identifier).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].container, installed);
        cleanup_orphaned_staging(&root).unwrap();
        assert!(!orphan.exists());
        assert!(!download.exists());
        assert!(backup.is_dir());
        assert_eq!(
            fs::read_to_string(backup.join("entry.js")).unwrap(),
            "backup"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_recovery_restores_previous_plugin_when_replacement_was_interrupted() {
        let root = plugin_test_root("reinstall-crash-rollback");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallcrashrollback";
        let target = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&target, identifier, "1.0.0", "old");
        let recovery = root.join(".plugin-recovery-fixture.transaction");
        fs::create_dir_all(&recovery).unwrap();
        fs::write(
            recovery.join(PLUGIN_REPLACEMENT_JOURNAL),
            serde_json::to_vec(&PluginReplacementJournal {
                previous_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_digest: [0; 32],
            })
            .unwrap(),
        )
        .unwrap();
        fs::rename(&target, recovery.join(PLUGIN_REPLACEMENT_BACKUP)).unwrap();

        cleanup_orphaned_staging(&root).unwrap();
        cleanup_orphaned_staging(&root).unwrap();

        assert_eq!(fs::read_to_string(target.join("entry.js")).unwrap(), "old");
        assert!(!recovery.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_recovery_discards_pre_rename_transactions_without_a_backup() {
        let root = plugin_test_root("reinstall-crash-before-journal");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let missing_journal = root.join(".plugin-recovery-missing.transaction");
        let partial_journal = root.join(".plugin-recovery-partial.transaction");
        fs::create_dir_all(&missing_journal).unwrap();
        fs::create_dir_all(&partial_journal).unwrap();
        fs::write(partial_journal.join(PLUGIN_REPLACEMENT_JOURNAL), b"{").unwrap();

        cleanup_orphaned_staging(&root).unwrap();

        assert!(!missing_journal.exists());
        assert!(!partial_journal.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_recovery_keeps_committed_replacement_and_discards_the_backup() {
        let root = plugin_test_root("reinstall-crash-commit");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallcrashcommit";
        let target = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&target, identifier, "1.0.0", "old");
        let replacement_source = root.join("replacement-source");
        write_plugin_fixture(&replacement_source, identifier, "2.0.0", "new");
        let replacement_digest = plugin_tree_digest(&replacement_source).unwrap();
        let recovery = root.join(".plugin-recovery-fixture.transaction");
        fs::create_dir_all(&recovery).unwrap();
        fs::write(
            recovery.join(PLUGIN_REPLACEMENT_JOURNAL),
            serde_json::to_vec(&PluginReplacementJournal {
                previous_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_digest,
            })
            .unwrap(),
        )
        .unwrap();
        fs::rename(&target, recovery.join(PLUGIN_REPLACEMENT_BACKUP)).unwrap();
        fs::rename(&replacement_source, &target).unwrap();

        cleanup_orphaned_staging(&root).unwrap();

        assert_eq!(fs::read_to_string(target.join("entry.js")).unwrap(), "new");
        assert!(!recovery.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_recovery_restores_backup_when_committed_replacement_is_incomplete() {
        let root = plugin_test_root("reinstall-crash-incomplete-commit");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallcrashincomplete";
        let target = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&target, identifier, "1.0.0", "old");
        let replacement_source = root.join("replacement-source");
        write_plugin_fixture(&replacement_source, identifier, "2.0.0", "new");
        let replacement_digest = plugin_tree_digest(&replacement_source).unwrap();
        let recovery = root.join(".plugin-recovery-fixture.transaction");
        fs::create_dir_all(&recovery).unwrap();
        fs::write(
            recovery.join(PLUGIN_REPLACEMENT_JOURNAL),
            serde_json::to_vec(&PluginReplacementJournal {
                previous_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_digest,
            })
            .unwrap(),
        )
        .unwrap();
        fs::rename(&target, recovery.join(PLUGIN_REPLACEMENT_BACKUP)).unwrap();
        fs::rename(&replacement_source, &target).unwrap();
        fs::write(target.join("entry.js"), "incomplete").unwrap();

        cleanup_orphaned_staging(&root).unwrap();

        assert_eq!(fs::read_to_string(target.join("entry.js")).unwrap(), "old");
        assert!(!recovery.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_new_replacement_finalizes_the_previous_transaction_for_the_same_target_first() {
        let root = plugin_test_root("reinstall-consecutive-transactions");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallconsecutive";
        let target = canonical_plugin_target(&root, identifier);
        write_plugin_fixture(&target, identifier, "1.0.0", "version-a");
        let version_b = root.join("version-b-source");
        write_plugin_fixture(&version_b, identifier, "2.0.0", "version-b");
        let version_b_digest = plugin_tree_digest(&version_b).unwrap();
        let old_recovery = root.join(".plugin-recovery-old.transaction");
        fs::create_dir_all(&old_recovery).unwrap();
        fs::write(
            old_recovery.join(PLUGIN_REPLACEMENT_JOURNAL),
            serde_json::to_vec(&PluginReplacementJournal {
                previous_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_digest: version_b_digest,
            })
            .unwrap(),
        )
        .unwrap();
        fs::rename(&target, old_recovery.join(PLUGIN_REPLACEMENT_BACKUP)).unwrap();
        fs::rename(&version_b, &target).unwrap();
        let version_c = root.join("version-c-source");
        write_plugin_fixture(&version_c, identifier, "3.0.0", "version-c");

        replace_plugin_entry(&version_c, &target, &target).unwrap();

        assert_eq!(
            fs::read_to_string(target.join("entry.js")).unwrap(),
            "version-c"
        );
        assert!(!fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(PLUGIN_REPLACEMENT_RECOVERY_PREFIX)
            }));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn startup_recovery_restores_an_interrupted_development_link_without_touching_source() {
        use std::os::unix::fs::symlink;

        let root = plugin_test_root("reinstall-crash-development-link");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifier = "io.iina.reinstallcrashdevelopment";
        let source = root.join("development-source");
        write_plugin_fixture(&source, identifier, "1.0.0", "development");
        let link = root.join("development.iinaplugin-dev");
        let target = canonical_plugin_target(&root, identifier);
        symlink(&source, &link).unwrap();
        let recovery = root.join(".plugin-recovery-fixture.transaction");
        fs::create_dir_all(&recovery).unwrap();
        fs::write(
            recovery.join(PLUGIN_REPLACEMENT_JOURNAL),
            serde_json::to_vec(&PluginReplacementJournal {
                previous_name: link.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_name: target.file_name().unwrap().to_string_lossy().into_owned(),
                replacement_digest: [0; 32],
            })
            .unwrap(),
        )
        .unwrap();
        fs::rename(&link, recovery.join(PLUGIN_REPLACEMENT_BACKUP)).unwrap();

        cleanup_orphaned_staging(&root).unwrap();

        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(source.join("entry.js")).unwrap(),
            "development"
        );
        assert!(!target.exists());
        assert!(!recovery.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validates_reverse_domain_identifiers() {
        assert!(is_valid_identifier("io.iina.demo"));
        assert!(is_valid_identifier("com.example.plugin_2"));
        assert!(!is_valid_identifier("plugin"));
        assert!(!is_valid_identifier("io..demo"));
        assert!(!is_valid_identifier("io.iina/plugin"));
    }

    #[test]
    fn standardizes_only_plain_github_owner_repository_sources() {
        assert_eq!(
            standardize_github_source("iina/plugin-demo").unwrap(),
            "iina/plugin-demo"
        );
        assert_eq!(
            standardize_github_source("https://github.com/iina/plugin-demo/").unwrap(),
            "iina/plugin-demo"
        );
        assert!(standardize_github_source("http://github.com/iina/plugin-demo").is_err());
        assert!(
            standardize_github_source("https://github.com/iina/plugin-demo/tree/main").is_err()
        );
        assert!(standardize_github_source("https://example.com/iina/plugin-demo").is_err());
    }

    #[test]
    fn validates_github_manifest_identity_and_version() {
        let manifest: PluginManifest = serde_json::from_str(
            r#"{
              "name": "GitHub Fixture",
              "author": {"name": "IINA"},
              "identifier": "io.iina.githubfixture",
              "version": "1.0.0",
              "entry": "entry.js",
              "ghRepo": "iina/plugin-demo",
              "ghVersion": 2
            }"#,
        )
        .unwrap();
        assert!(validate_github_manifest(&manifest, "iina/plugin-demo").is_ok());
        assert!(validate_github_manifest(&manifest, "iina/other").is_err());
    }

    #[test]
    fn accepts_only_trusted_github_release_asset_hosts() {
        assert!(validate_github_download_url(
            "https://github.com/iina/demo/releases/download/v1/demo.iinaplgz"
        )
        .is_ok());
        assert!(
            validate_github_download_url("https://objects.githubusercontent.com/fixture").is_ok()
        );
        assert!(validate_github_download_url("https://example.com/demo.iinaplgz").is_err());
        assert!(validate_github_download_url("http://github.com/demo.iinaplgz").is_err());
    }

    #[test]
    fn rejects_unsafe_plugin_paths() {
        let root = std::env::temp_dir().join(format!("iima-plugin-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("entry.js"), "").unwrap();

        assert!(validate_local_plugin_path("entry.js", &root, "entry").is_ok());
        assert!(validate_local_plugin_path("../entry.js", &root, "entry").is_err());
        assert!(validate_local_plugin_path("/tmp/entry.js", &root, "entry").is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_bounded_local_plugin_pages() {
        let root =
            std::env::temp_dir().join(format!("iima-plugin-page-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("preferences.html"),
            "<input data-pref-key=\"enabled\">",
        )
        .unwrap();

        assert_eq!(
            read_plugin_page(&root, "preferences.html", "preferencesPage").unwrap(),
            "<input data-pref-key=\"enabled\">"
        );
        assert!(read_plugin_page(&root, "../preferences.html", "preferencesPage").is_err());
        fs::write(
            root.join("large.html"),
            vec![b'x'; MAX_PLUGIN_PAGE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(read_plugin_page(&root, "large.html", "preferencesPage").is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_private_plugin_download_destinations_without_path_escape() {
        let root = std::env::temp_dir().join(format!(
            "iima-plugin-download-path-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        assert_eq!(
            plugin_private_destination(&root, "io.iina.fixture", "@tmp/cache/file.bin", "@tmp/")
                .unwrap(),
            Some(
                root.join(PLUGIN_TEMP_DIRECTORY)
                    .join("io.iina.fixture")
                    .join("cache/file.bin")
            )
        );
        assert_eq!(
            plugin_private_destination(&root, "io.iina.fixture", "@data/settings.json", "@tmp/")
                .unwrap(),
            None
        );
        assert!(plugin_private_destination(
            &root,
            "io.iina.fixture",
            "@data/../../escape",
            "@data/"
        )
        .is_err());
        assert_eq!(
            plugin_private_destination(&root, "io.iina.fixture", "@tmp/", "@tmp/").unwrap(),
            Some(root.join(PLUGIN_TEMP_DIRECTORY).join("io.iina.fixture"))
        );
        assert_eq!(
            plugin_private_destination(&root, "io.iina.fixture", "@tmp/./cache/file.bin", "@tmp/")
                .unwrap(),
            Some(
                root.join(PLUGIN_TEMP_DIRECTORY)
                    .join("io.iina.fixture")
                    .join("cache/file.bin")
            )
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn standardizes_current_style_paths_without_resolving_symlinks() {
        assert_eq!(
            standardize_plugin_path(Path::new("/tmp/media/sub/../sidecar.srt")),
            PathBuf::from("/tmp/media/sidecar.srt")
        );
        assert_eq!(
            standardize_plugin_path(Path::new("/../../sidecar.srt")),
            PathBuf::from("/sidecar.srt")
        );
    }

    #[test]
    fn preserves_plugin_enable_state() {
        let root =
            std::env::temp_dir().join(format!("iima-plugin-state-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut state = PluginState::default();
        state.enabled.insert("io.iina.demo".to_string(), true);
        state.order.push("io.iina.demo".to_string());
        save_state(&root, &state).unwrap();

        assert_eq!(
            load_state(&root).unwrap().enabled.get("io.iina.demo"),
            Some(&true)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reorders_installed_plugins_persists_boundaries_and_rejects_invalid_moves_without_mutation() {
        let root = plugin_test_root("reorder");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let identifiers = [
            "io.iina.reorder.a",
            "io.iina.reorder.b",
            "io.iina.reorder.c",
            "io.iina.reorder.d",
        ];
        for identifier in identifiers {
            write_plugin_fixture(
                &root.join(format!("{identifier}.iinaplugin")),
                identifier,
                "1.0.0",
                "module.exports = true;",
            );
        }
        let owned_identifiers = |values: &[&str]| {
            values
                .iter()
                .map(|identifier| identifier.to_string())
                .collect::<Vec<_>>()
        };
        let mut state = PluginState::default();
        state.order = owned_identifiers(&identifiers);
        state.enabled = identifiers
            .iter()
            .map(|identifier| (identifier.to_string(), true))
            .collect();
        save_state(&root, &state).unwrap();
        let record_identifiers = |records: Vec<PluginRecord>| {
            records
                .into_iter()
                .map(|record| record.identifier)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            record_identifiers(reorder_in_root(&root, identifiers[0], 2).unwrap()),
            owned_identifiers(&[
                identifiers[1],
                identifiers[2],
                identifiers[0],
                identifiers[3],
            ])
        );
        assert_eq!(
            record_identifiers(reorder_in_root(&root, identifiers[3], 1).unwrap()),
            owned_identifiers(&[
                identifiers[1],
                identifiers[3],
                identifiers[2],
                identifiers[0],
            ])
        );
        assert_eq!(
            record_identifiers(reorder_in_root(&root, identifiers[0], 0).unwrap()),
            owned_identifiers(&[
                identifiers[0],
                identifiers[1],
                identifiers[3],
                identifiers[2],
            ])
        );
        let expected = owned_identifiers(&[
            identifiers[1],
            identifiers[3],
            identifiers[2],
            identifiers[0],
        ]);
        assert_eq!(
            record_identifiers(reorder_in_root(&root, identifiers[0], 3).unwrap()),
            expected
        );

        let persisted = load_state(&root).unwrap();
        assert_eq!(persisted.order, expected);
        assert_eq!(
            record_identifiers(scan_plugins(&root, &persisted).unwrap()),
            expected
        );
        let state_before_error = fs::read(root.join(PLUGIN_STATE_FILE)).unwrap();

        let missing_error = reorder_in_root(&root, "io.iina.reorder.missing", 0).unwrap_err();
        assert!(missing_error.contains("is not installed"));
        assert_eq!(
            fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(),
            state_before_error
        );
        assert_eq!(
            record_identifiers(scan_plugins(&root, &load_state(&root).unwrap()).unwrap()),
            expected
        );

        let range_error = reorder_in_root(&root, identifiers[1], identifiers.len()).unwrap_err();
        assert!(range_error.contains("destination index is out of range"));
        assert_eq!(
            fs::read(root.join(PLUGIN_STATE_FILE)).unwrap(),
            state_before_error
        );
        assert_eq!(
            record_identifiers(scan_plugins(&root, &load_state(&root).unwrap()).unwrap()),
            expected
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_spec_collects_only_package_javascript() {
        let root =
            std::env::temp_dir().join(format!("iima-plugin-runtime-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(
            root.join("Info.json"),
            r#"{
              "name": "Runtime Fixture",
              "author": {"name": "IINA"},
              "identifier": "io.iina.runtimefixture",
              "version": "1.0.0",
              "entry": "entry.js",
              "globalEntry": "global.js",
              "preferenceDefaults": {"enabled": true}
            }"#,
        )
        .unwrap();
        fs::write(root.join("entry.js"), "require('./lib/helper')").unwrap();
        fs::write(root.join("global.js"), "iina.console.log('global')").unwrap();
        fs::write(root.join("lib/helper.js"), "module.exports = 1").unwrap();
        fs::write(root.join("ignored.txt"), "not JavaScript").unwrap();

        let manifest = read_manifest(&root).unwrap();
        validate_manifest(&manifest, &root).unwrap();
        let spec = runtime_spec_from_manifest(&root, &manifest, false).unwrap();

        assert_eq!(spec.global_entry.as_deref(), Some("global.js"));
        assert_eq!(spec.scripts.len(), 3);
        assert_eq!(
            spec.scripts.get("lib/helper.js"),
            Some(&"module.exports = 1".to_string())
        );
        assert_eq!(
            spec.preference_defaults.get("enabled"),
            Some(&Value::Bool(true))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_duplicate_plugin_menu_ids_at_any_depth() {
        let items = vec![PluginMenuItemDefinition {
            id: "parent".to_string(),
            title: "Parent".to_string(),
            enabled: true,
            selected: false,
            key_binding: None,
            separator: false,
            items: vec![PluginMenuItemDefinition {
                id: "parent".to_string(),
                title: "Child".to_string(),
                enabled: true,
                selected: false,
                key_binding: None,
                separator: false,
                items: Vec::new(),
            }],
        }];

        assert!(validate_menu_items(&items).is_err());
    }

    #[test]
    fn matches_plugin_network_domains_without_suffix_confusion() {
        assert!(plugin_domain_matches("example.com", "example.com"));
        assert!(plugin_domain_matches("*.example.com", "cdn.example.com"));
        assert!(!plugin_domain_matches("*.example.com", "example.com"));
        assert!(!plugin_domain_matches("*.example.com", "notexample.com"));
        assert!(!plugin_domain_matches("example.com", "cdn.example.com"));
    }

    #[test]
    fn installs_enables_and_removes_a_valid_plugin_package() {
        let root =
            std::env::temp_dir().join(format!("iima-plugin-package-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("Info.json"),
            r#"{
              "name": "Fixture Plugin",
              "author": {"name": "IINA"},
              "identifier": "io.iina.fixture",
              "version": "1.0.0",
              "entry": "entry.js",
              "permissions": ["show-osd", "network-request"],
              "allowedDomains": ["example.com"],
              "subtitleProviders": [{"id": "fixture", "name": "Fixture Provider"}]
            }"#,
        )
        .unwrap();
        fs::write(source.join("entry.js"), "console.log('fixture');").unwrap();
        let package = root.join("fixture.iinaplgz");
        let output = Command::new("/usr/bin/zip")
            .args(["-q", "-r"])
            .arg(&package)
            .arg("source")
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success());

        let record = install_package_in_root(&root, &package, None, None).unwrap();
        assert_eq!(record.identifier, "io.iina.fixture");
        assert!(!record.enabled);
        assert_eq!(record.subtitle_providers.len(), 1);
        assert!(root.join("io.iina.fixture.iinaplugin").is_dir());

        let enabled = set_enabled_in_root(&root, "io.iina.fixture", true).unwrap();
        assert_eq!(enabled.len(), 1);
        assert!(enabled[0].enabled);

        assert!(remove_from_root(&root, "io.iina.fixture")
            .unwrap()
            .is_empty());
        assert!(!root.join("io.iina.fixture.iinaplugin").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn github_update_replaces_only_the_matching_plugin_and_preserves_state() {
        let root = std::env::temp_dir().join(format!(
            "iima-plugin-github-update-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let installed = root.join("io.iina.githubfixture.iinaplugin");
        fs::create_dir_all(&installed).unwrap();
        fs::write(
            installed.join("Info.json"),
            r#"{
              "name": "GitHub Fixture",
              "author": {"name": "IINA"},
              "identifier": "io.iina.githubfixture",
              "version": "1.0.0",
              "entry": "entry.js",
              "ghRepo": "iina/plugin-demo",
              "ghVersion": 1
            }"#,
        )
        .unwrap();
        fs::write(installed.join("entry.js"), "module.exports = 'old';").unwrap();
        let mut state = PluginState::default();
        state
            .enabled
            .insert("io.iina.githubfixture".to_string(), true);
        state.order.push("io.iina.githubfixture".to_string());
        save_state(&root, &state).unwrap();

        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("Info.json"),
            r#"{
              "name": "GitHub Fixture",
              "author": {"name": "IINA"},
              "identifier": "io.iina.githubfixture",
              "version": "2.0.0",
              "entry": "entry.js",
              "ghRepo": "iina/plugin-demo",
              "ghVersion": 2
            }"#,
        )
        .unwrap();
        fs::write(source.join("entry.js"), "module.exports = 'new';").unwrap();
        let package = root.join("update.iinaplgz");
        let output = Command::new("/usr/bin/zip")
            .args(["-q", "-r"])
            .arg(&package)
            .arg("source")
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success());

        let record = install_package_in_root(
            &root,
            &package,
            Some("iina/plugin-demo"),
            Some("io.iina.githubfixture"),
        )
        .unwrap();
        assert_eq!(record.version, "2.0.0");
        assert!(record.enabled);
        assert_eq!(record.github_version, Some(2));
        assert_eq!(
            fs::read_to_string(installed.join("entry.js")).unwrap(),
            "module.exports = 'new';"
        );
        let state = load_state(&root).unwrap();
        assert_eq!(state.enabled.get("io.iina.githubfixture"), Some(&true));
        assert_eq!(state.order, vec!["io.iina.githubfixture".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn development_plugin_link_is_external_and_only_the_link_is_removed() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "iima-plugin-development-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("development-source");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("Info.json"),
            r#"{
              "name": "Development Fixture",
              "author": {"name": "IINA"},
              "identifier": "io.iina.developmentfixture",
              "version": "1.0.0",
              "entry": "entry.js"
            }"#,
        )
        .unwrap();
        fs::write(
            source.join("implementation.js"),
            "module.exports = 'development';",
        )
        .unwrap();
        symlink(source.join("implementation.js"), source.join("entry.js")).unwrap();
        let link = root.join("development.iinaplugin-dev");
        symlink(&source, &link).unwrap();

        let mut state = PluginState::default();
        state
            .enabled
            .insert("io.iina.developmentfixture".to_string(), true);
        state.order.push("io.iina.developmentfixture".to_string());
        save_state(&root, &state).unwrap();

        let records = scan_plugins(&root, &state).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].is_external);
        let plugin = plugin_root_for_identifier(&root, "io.iina.developmentfixture").unwrap();
        let manifest = read_manifest(&plugin.root).unwrap();
        let spec = runtime_spec_from_manifest(&plugin.root, &manifest, true).unwrap();
        assert_eq!(
            spec.scripts.get("entry.js"),
            Some(&"module.exports = 'development';".to_string())
        );

        assert!(remove_from_root(&root, "io.iina.developmentfixture")
            .unwrap()
            .is_empty());
        assert!(!link.exists());
        assert!(source.is_dir());
        assert!(source.join("Info.json").is_file());
        let _ = fs::remove_dir_all(root);
    }
}
