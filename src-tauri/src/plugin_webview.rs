use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tauri::http::{
    header::{self},
    Method, Request, Response, StatusCode,
};
use tauri::{
    AppHandle, Emitter, Manager, Runtime, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::plugins;

pub const PLUGIN_WEBVIEW_SCHEME: &str = "iima-plugin";
pub const PLUGIN_WEBVIEW_MESSAGE_EVENT: &str = "iima-plugin-webview-message";

const SIMPLE_PAGE_NAME: &str = "__iima_simple__.html";
const BRIDGE_ENDPOINT: &str = "__iima_bridge__";
const STANDALONE_LABEL_PREFIX: &str = "plugin-window-";
const MAX_ACTIVE_PAGE_GRANTS: usize = 96;
const PAGE_GRANT_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
const MAX_RESOURCE_COUNT: usize = 256;
const MAX_RESOURCE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PAGE_BYTES: u64 = 512 * 1024;
const MAX_TOTAL_RESOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_BRIDGE_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_MESSAGE_NAME_BYTES: usize = 256;
const MAX_WINDOW_DIMENSION: f64 = 16_384.0;
const PLUGIN_PAGE_LOCKDOWN_SCRIPT: &str = r#"
(() => {
  const deny = () => Promise.reject(new Error("Tauri IPC is unavailable in plugin pages"));
  try { if (window.__TAURI__?.core) window.__TAURI__.core.invoke = deny; } catch (_) {}
  try { if (window.__TAURI_INTERNALS__) window.__TAURI_INTERNALS__.invoke = deny; } catch (_) {}
  try { if (window.ipc) window.ipc.postMessage = deny; } catch (_) {}
  try { Reflect.deleteProperty(window, "__TAURI__"); } catch (_) {}
  try { Reflect.deleteProperty(window, "__TAURI_INTERNALS__"); } catch (_) {}
  try { Reflect.deleteProperty(window, "ipc"); } catch (_) {}
})();
"#;

static TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static PAGE_GRANTS: OnceLock<Mutex<BTreeMap<String, PageGrant>>> = OnceLock::new();

#[derive(Debug, Clone)]
struct PageGrant {
    identifier: String,
    role: String,
    plugin_name: String,
    owner_window_label: String,
    expected_webview_label: String,
    surface: String,
    root: PathBuf,
    simple_mode: bool,
    allowed_domains: Vec<String>,
    created_at: Instant,
    resources: BTreeMap<String, u64>,
    total_resource_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWebviewPage {
    pub token: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct PageBridgeRequest {
    name: String,
    #[serde(default)]
    data: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageBridgeEvent {
    identifier: String,
    role: String,
    surface: String,
    token: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

fn grants() -> &'static Mutex<BTreeMap<String, PageGrant>> {
    PAGE_GRANTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[tauri::command]
pub fn plugin_webview_prepare_page(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
    surface: String,
    path: Option<String>,
    simple_mode: bool,
) -> Result<PluginWebviewPage, String> {
    let role = validated_instance_role(&role)?;
    let surface = validated_surface(&surface)?;
    if surface != "standalone" && role != "entry" {
        return Err("Only entry plugin instances own overlay and sidebar pages".to_string());
    }
    if simple_mode == path.is_some() {
        return Err(
            "A plugin WebView page must select either simple mode or one package file".to_string(),
        );
    }
    let access = plugins::plugin_webview_access(&app, &identifier, surface, path.as_deref())?;
    let owner_window_label = window.label().to_string();
    let expected_webview_label = if surface == "standalone" {
        standalone_window_label(&owner_window_label, &identifier, role)
    } else {
        owner_window_label.clone()
    };
    let token = new_page_token()?;
    let page_path = if simple_mode {
        SIMPLE_PAGE_NAME.to_string()
    } else {
        normalize_relative_path(path.as_deref().unwrap_or_default())?
    };
    let url = format!(
        "{PLUGIN_WEBVIEW_SCHEME}://{token}.localhost/{token}/{}",
        encode_uri_path(&page_path)
    );
    let grant = PageGrant {
        identifier: identifier.clone(),
        role: role.to_string(),
        plugin_name: access.plugin_name,
        owner_window_label: owner_window_label.clone(),
        expected_webview_label,
        surface: surface.to_string(),
        root: access.root,
        simple_mode,
        allowed_domains: access.allowed_domains,
        created_at: Instant::now(),
        resources: BTreeMap::new(),
        total_resource_bytes: 0,
    };
    let mut entries = grants().lock().map_err(|error| error.to_string())?;
    remove_expired_grants(&mut entries);
    entries.retain(|_, existing| {
        existing.owner_window_label != owner_window_label
            || existing.identifier != identifier
            || existing.role != role
            || existing.surface != surface
    });
    if entries.len() >= MAX_ACTIVE_PAGE_GRANTS {
        return Err("Too many plugin WebView pages are active".to_string());
    }
    entries.insert(token.clone(), grant);
    Ok(PluginWebviewPage { token, url })
}

#[tauri::command]
pub fn plugin_standalone_window_load(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
    token: String,
    url: String,
) -> Result<(), String> {
    let role = validated_instance_role(&role)?;
    let owner = window.label();
    let grant = require_grant(&token, owner, &identifier, role, "standalone")?;
    validate_page_url(&url, &token)?;
    let page_url = Url::parse(&url).map_err(|error| error.to_string())?;
    let label = standalone_window_label(owner, &identifier, role);
    if label != grant.expected_webview_label {
        return Err("Standalone plugin window authorization is inconsistent".to_string());
    }
    if let Some(standalone) = app.get_webview_window(&label) {
        standalone
            .navigate(page_url)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    let navigation_owner = owner.to_string();
    let navigation_identifier = identifier.clone();
    let navigation_role = role.to_string();
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::CustomProtocol(page_url))
        .title(format!("Window — {}", grant.plugin_name))
        .inner_size(600.0, 400.0)
        .resizable(true)
        .decorations(true)
        .visible(false)
        .center()
        .initialization_script(PLUGIN_PAGE_LOCKDOWN_SCRIPT)
        .on_navigation(move |candidate| {
            active_standalone_url(
                candidate,
                &navigation_owner,
                &navigation_identifier,
                &navigation_role,
            )
        })
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn plugin_standalone_window_open(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
) -> Result<(), String> {
    let role = validated_instance_role(&role)?;
    let label = standalone_window_label(window.label(), &identifier, role);
    let standalone = app
        .get_webview_window(&label)
        .ok_or_else(|| "Standalone plugin window has not loaded a page".to_string())?;
    standalone.unminimize().map_err(|error| error.to_string())?;
    standalone.show().map_err(|error| error.to_string())?;
    standalone.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn plugin_standalone_window_close(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
) -> Result<(), String> {
    let role = validated_instance_role(&role)?;
    let label = standalone_window_label(window.label(), &identifier, role);
    if let Some(standalone) = app.get_webview_window(&label) {
        standalone.hide().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn plugin_standalone_window_is_open(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
) -> Result<bool, String> {
    let role = validated_instance_role(&role)?;
    let label = standalone_window_label(window.label(), &identifier, role);
    app.get_webview_window(&label)
        .map(|standalone| standalone.is_visible().map_err(|error| error.to_string()))
        .transpose()
        .map(|visible| visible.unwrap_or(false))
}

#[tauri::command]
pub fn plugin_standalone_window_set_property(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
    token: String,
    properties: Map<String, Value>,
) -> Result<(), String> {
    let role = validated_instance_role(&role)?;
    let grant = require_grant(&token, window.label(), &identifier, role, "standalone")?;
    let standalone = app
        .get_webview_window(&grant.expected_webview_label)
        .ok_or_else(|| "Standalone plugin window has not loaded a page".to_string())?;

    if let Some(title) = properties.get("title").and_then(Value::as_str) {
        if title.len() > 512 || title.contains('\0') {
            return Err("Standalone plugin window title is invalid".to_string());
        }
        standalone
            .set_title(&format!("{title} — {}", grant.plugin_name))
            .map_err(|error| error.to_string())?;
    }
    if let Some(resizable) = properties.get("resizable").and_then(Value::as_bool) {
        standalone
            .set_resizable(resizable)
            .map_err(|error| error.to_string())?;
    }

    let full_size_content_view = properties
        .get("fullSizeContentView")
        .and_then(Value::as_bool);
    let hide_title_bar = properties.get("hideTitleBar").and_then(Value::as_bool);
    if full_size_content_view.is_some() || hide_title_bar.is_some() {
        configure_native_plugin_window(&standalone, full_size_content_view, hide_title_bar)?;
    }
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn plugin_standalone_window_set_frame(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
    token: String,
    width: Option<f64>,
    height: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    let role = validated_instance_role(&role)?;
    let grant = require_grant(&token, window.label(), &identifier, role, "standalone")?;
    let standalone = app
        .get_webview_window(&grant.expected_webview_label)
        .ok_or_else(|| "Standalone plugin window has not loaded a page".to_string())?;
    for (label, value, positive) in [
        ("width", width, true),
        ("height", height, true),
        ("x", x, false),
        ("y", y, false),
    ] {
        if let Some(value) = value {
            if !value.is_finite()
                || (positive && !(0.0..=MAX_WINDOW_DIMENSION).contains(&value))
                || (positive && value == 0.0)
            {
                return Err(format!("Standalone plugin window {label} is invalid"));
            }
        }
    }
    set_native_plugin_window_frame(&standalone, width, height, x, y)
}

#[tauri::command]
pub fn plugin_standalone_window_post_message(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
    token: String,
    name: String,
    data: Option<Value>,
) -> Result<(), String> {
    validate_message_name(&name)?;
    let role = validated_instance_role(&role)?;
    let grant = require_grant(&token, window.label(), &identifier, role, "standalone")?;
    let standalone = app
        .get_webview_window(&grant.expected_webview_label)
        .ok_or_else(|| "Standalone plugin window has not loaded a page".to_string())?;
    let name = serde_json::to_string(&name).map_err(|error| error.to_string())?;
    let data = data
        .map(|value| serde_json::to_string(&value))
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| "undefined".to_string());
    standalone
        .eval(&format!("window.iina?._emit({name}, {data});"))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn plugin_standalone_window_set_simple_value(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
    token: String,
    target: String,
    value: String,
) -> Result<(), String> {
    let role = validated_instance_role(&role)?;
    let grant = require_grant(&token, window.label(), &identifier, role, "standalone")?;
    if !grant.simple_mode {
        return Err("Standalone window style and content require simple mode".to_string());
    }
    let method = match target.as_str() {
        "style" => "_simpleModeSetStyle",
        "content" => "_simpleModeSetContent",
        _ => return Err("Unknown standalone simple-mode target".to_string()),
    };
    let standalone = app
        .get_webview_window(&grant.expected_webview_label)
        .ok_or_else(|| "Standalone plugin window has not loaded a page".to_string())?;
    let value = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    standalone
        .eval(&format!("window.iina?.{method}({value});"))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn plugin_webview_cleanup(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
) -> Result<(), String> {
    cleanup_plugin(&app, window.label(), &identifier)
}

#[tauri::command]
pub fn plugin_webview_cleanup_role(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
) -> Result<(), String> {
    let role = validated_instance_role(&role)?;
    cleanup_plugin_role(&app, window.label(), &identifier, role)
}

pub fn cleanup_plugin_role<R: Runtime>(
    app: &AppHandle<R>,
    owner_window_label: &str,
    identifier: &str,
    role: &str,
) -> Result<(), String> {
    let mut entries = grants().lock().map_err(|error| error.to_string())?;
    entries.retain(|_, grant| {
        grant.owner_window_label != owner_window_label
            || grant.identifier != identifier
            || grant.role != role
    });
    drop(entries);
    let label = standalone_window_label(owner_window_label, identifier, role);
    if let Some(window) = app.get_webview_window(&label) {
        window.destroy().map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn cleanup_plugin<R: Runtime>(
    app: &AppHandle<R>,
    owner_window_label: &str,
    identifier: &str,
) -> Result<(), String> {
    let mut entries = grants().lock().map_err(|error| error.to_string())?;
    let labels = entries
        .values()
        .filter(|grant| {
            grant.owner_window_label == owner_window_label && grant.identifier == identifier
        })
        .map(|grant| grant.expected_webview_label.clone())
        .collect::<Vec<_>>();
    entries.retain(|_, grant| {
        grant.owner_window_label != owner_window_label || grant.identifier != identifier
    });
    drop(entries);
    for label in labels {
        if label.starts_with(STANDALONE_LABEL_PREFIX) {
            if let Some(window) = app.get_webview_window(&label) {
                window.destroy().map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

pub fn cleanup_owner<R: Runtime>(app: &AppHandle<R>, owner_window_label: &str) {
    if let Ok(mut entries) = grants().lock() {
        let labels = entries
            .values()
            .filter(|grant| grant.owner_window_label == owner_window_label)
            .map(|grant| grant.expected_webview_label.clone())
            .collect::<Vec<_>>();
        entries.retain(|_, grant| grant.owner_window_label != owner_window_label);
        drop(entries);
        for label in labels {
            if label.starts_with(STANDALONE_LABEL_PREFIX) {
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.destroy();
                }
            }
        }
    }
}

pub fn cleanup_all<R: Runtime>(app: &AppHandle<R>) {
    let labels = grants()
        .lock()
        .map(|mut entries| {
            let labels = entries
                .values()
                .map(|grant| grant.expected_webview_label.clone())
                .collect::<Vec<_>>();
            entries.clear();
            labels
        })
        .unwrap_or_default();
    for label in labels {
        if label.starts_with(STANDALONE_LABEL_PREFIX) {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.destroy();
            }
        }
    }
}

pub fn is_standalone_window_label(label: &str) -> bool {
    label.starts_with(STANDALONE_LABEL_PREFIX)
}

pub fn handle_protocol<R: Runtime>(
    context: tauri::UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    match protocol_response(&context, &request) {
        Ok(response) => response,
        Err((status, message)) => error_response(status, &message),
    }
}

fn protocol_response<R: Runtime>(
    context: &tauri::UriSchemeContext<'_, R>,
    request: &Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, (StatusCode, String)> {
    let (token, relative) = protocol_request_parts(request.uri().path())?;
    if request.uri().host() != Some(format!("{token}.localhost").as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Plugin resource origin does not match its authorization".to_string(),
        ));
    }
    let grant = require_protocol_grant(&token, context.webview_label())?;

    if relative == BRIDGE_ENDPOINT {
        if request.method() != Method::POST {
            return Err((
                StatusCode::METHOD_NOT_ALLOWED,
                "Plugin page bridge accepts POST only".to_string(),
            ));
        }
        return handle_page_message(context, request.body(), &token, &grant);
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return Err((
            StatusCode::METHOD_NOT_ALLOWED,
            "Plugin resources accept GET and HEAD only".to_string(),
        ));
    }

    let (mut bytes, content_type, source_bytes) = if grant.simple_mode {
        if relative != SIMPLE_PAGE_NAME {
            return Err((
                StatusCode::NOT_FOUND,
                "Plugin resource not found".to_string(),
            ));
        }
        let html = simple_mode_html(&grant.surface);
        let source_bytes = u64::try_from(html.len()).unwrap_or(u64::MAX);
        (
            inject_bridge(html, &token, &grant.allowed_domains),
            "text/html; charset=utf-8",
            source_bytes,
        )
    } else {
        let normalized = normalize_relative_path(&relative)
            .map_err(|message| (StatusCode::BAD_REQUEST, message))?;
        let path = secure_resource_path(&grant.root, &normalized)?;
        let metadata = fs::metadata(&path).map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                "Plugin resource not found".to_string(),
            )
        })?;
        if !metadata.is_file() {
            return Err((
                StatusCode::NOT_FOUND,
                "Plugin resource not found".to_string(),
            ));
        }
        let content_type = content_type_for_path(&path);
        let limit = if content_type.starts_with("text/html") {
            MAX_PAGE_BYTES
        } else {
            MAX_RESOURCE_BYTES
        };
        if metadata.len() > limit {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "Plugin WebView resource exceeds its size limit".to_string(),
            ));
        }
        let source = fs::read(&path).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to read plugin resource".to_string(),
            )
        })?;
        let bytes = if content_type.starts_with("text/html") {
            let html = String::from_utf8(source).map_err(|_| {
                (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "Plugin HTML must use UTF-8".to_string(),
                )
            })?;
            inject_bridge(&html, &token, &grant.allowed_domains)
        } else {
            source
        };
        (bytes, content_type, metadata.len())
    };

    account_resource(&token, &relative, source_bytes)?;
    if request.method() == Method::HEAD {
        bytes.clear();
    }
    response_builder(StatusCode::OK, content_type)
        .body(bytes)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

fn handle_page_message<R: Runtime>(
    context: &tauri::UriSchemeContext<'_, R>,
    body: &[u8],
    token: &str,
    grant: &PageGrant,
) -> Result<Response<Vec<u8>>, (StatusCode, String)> {
    if body.len() > MAX_BRIDGE_MESSAGE_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "Plugin page message exceeds 256 KiB".to_string(),
        ));
    }
    let message: PageBridgeRequest = serde_json::from_slice(body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid plugin page message".to_string(),
        )
    })?;
    validate_message_name(&message.name).map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    context
        .app_handle()
        .emit_to(
            &grant.owner_window_label,
            PLUGIN_WEBVIEW_MESSAGE_EVENT,
            PageBridgeEvent {
                identifier: grant.identifier.clone(),
                role: grant.role.clone(),
                surface: grant.surface.clone(),
                token: token.to_string(),
                name: message.name,
                data: message.data,
            },
        )
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    response_builder(StatusCode::NO_CONTENT, "text/plain; charset=utf-8")
        .body(Vec::new())
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

fn response_builder(
    status: StatusCode,
    content_type: &'static str,
) -> tauri::http::response::Builder {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
}

fn error_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    response_builder(status, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn require_protocol_grant(
    token: &str,
    requesting_webview_label: &str,
) -> Result<PageGrant, (StatusCode, String)> {
    let mut entries = grants().lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Plugin WebView authorization is unavailable".to_string(),
        )
    })?;
    remove_expired_grants(&mut entries);
    let grant = entries.get(token).ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Plugin page authorization expired".to_string(),
        )
    })?;
    if grant.expected_webview_label != requesting_webview_label {
        return Err((
            StatusCode::FORBIDDEN,
            "Plugin page authorization belongs to another WebView".to_string(),
        ));
    }
    Ok(grant.clone())
}

fn require_grant(
    token: &str,
    owner_window_label: &str,
    identifier: &str,
    role: &str,
    surface: &str,
) -> Result<PageGrant, String> {
    let mut entries = grants().lock().map_err(|error| error.to_string())?;
    remove_expired_grants(&mut entries);
    let grant = entries
        .get(token)
        .ok_or_else(|| "Plugin page authorization expired".to_string())?;
    if grant.owner_window_label != owner_window_label
        || grant.identifier != identifier
        || grant.role != role
        || grant.surface != surface
    {
        return Err("Plugin page authorization does not match its owner".to_string());
    }
    Ok(grant.clone())
}

fn account_resource(token: &str, relative: &str, bytes: u64) -> Result<(), (StatusCode, String)> {
    let mut entries = grants().lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Plugin WebView resource accounting is unavailable".to_string(),
        )
    })?;
    let grant = entries.get_mut(token).ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Plugin page authorization expired".to_string(),
        )
    })?;
    account_resource_in_grant(grant, relative, bytes)
}

fn account_resource_in_grant(
    grant: &mut PageGrant,
    relative: &str,
    bytes: u64,
) -> Result<(), (StatusCode, String)> {
    if grant.resources.contains_key(relative) {
        return Ok(());
    }
    if grant.resources.len() >= MAX_RESOURCE_COUNT {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Plugin page requested too many distinct resources".to_string(),
        ));
    }
    let total = grant.total_resource_bytes.saturating_add(bytes);
    if total > MAX_TOTAL_RESOURCE_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "Plugin page resources exceed the 16 MiB total limit".to_string(),
        ));
    }
    grant.resources.insert(relative.to_string(), bytes);
    grant.total_resource_bytes = total;
    Ok(())
}

fn remove_expired_grants(entries: &mut BTreeMap<String, PageGrant>) {
    entries.retain(|_, grant| grant.created_at.elapsed() <= PAGE_GRANT_LIFETIME);
}

fn protocol_request_parts(path: &str) -> Result<(String, String), (StatusCode, String)> {
    let raw = path.trim_start_matches('/');
    let (token, relative) = raw.split_once('/').ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Plugin resource URL is incomplete".to_string(),
        )
    })?;
    if token.len() != 48 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Plugin resource token is invalid".to_string(),
        ));
    }
    let relative =
        decode_uri_path(relative).map_err(|message| (StatusCode::BAD_REQUEST, message))?;
    Ok((token.to_ascii_lowercase(), relative))
}

fn secure_resource_path(root: &Path, relative: &str) -> Result<PathBuf, (StatusCode, String)> {
    let candidate = root.join(relative);
    let resolved = fs::canonicalize(&candidate).map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            "Plugin resource not found".to_string(),
        )
    })?;
    if !resolved.starts_with(root) {
        return Err((
            StatusCode::FORBIDDEN,
            "Plugin resource escapes its package".to_string(),
        ));
    }
    Ok(resolved)
}

fn validated_surface(surface: &str) -> Result<&str, String> {
    match surface {
        "overlay" | "sidebar" | "standalone" => Ok(surface),
        _ => Err("Unknown plugin WebView surface".to_string()),
    }
}

fn validated_instance_role(role: &str) -> Result<&str, String> {
    match role {
        "entry" | "global" => Ok(role),
        _ => Err("Plugin WebView role must be entry or global".to_string()),
    }
}

fn normalize_relative_path(raw: &str) -> Result<String, String> {
    if raw.is_empty() || raw.contains('\0') {
        return Err("Plugin WebView path is empty or invalid".to_string());
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Plugin WebView path must stay inside its package".to_string());
    }
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Err("Plugin WebView path is empty".to_string());
    }
    Ok(components.join("/"))
}

fn validate_page_url(url: &str, token: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|error| error.to_string())?;
    if parsed.scheme() != PLUGIN_WEBVIEW_SCHEME
        || parsed.host_str() != Some(format!("{token}.localhost").as_str())
    {
        return Err("Standalone plugin window URL uses an invalid protocol".to_string());
    }
    let url_token = parsed
        .path_segments()
        .and_then(|mut segments| segments.next())
        .unwrap_or_default();
    if url_token != token {
        return Err("Standalone plugin window URL has the wrong authorization".to_string());
    }
    Ok(())
}

fn active_standalone_url(url: &Url, owner: &str, identifier: &str, role: &str) -> bool {
    if url.scheme() != PLUGIN_WEBVIEW_SCHEME {
        return false;
    }
    let Some(token) = url
        .path_segments()
        .and_then(|mut segments| segments.next().map(str::to_string))
    else {
        return false;
    };
    url.host_str() == Some(format!("{token}.localhost").as_str())
        && require_grant(&token, owner, identifier, role, "standalone").is_ok()
}

fn validate_message_name(name: &str) -> Result<(), String> {
    if name.len() > MAX_MESSAGE_NAME_BYTES || name.contains('\0') {
        return Err("Plugin page message name is invalid".to_string());
    }
    Ok(())
}

fn new_page_token() -> Result<String, String> {
    let mut random = [0_u8; 24];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .or_else(|_| {
            let sequence = TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let digest = Sha256::digest(format!("{timestamp}:{sequence}:{}", std::process::id()));
            random.copy_from_slice(&digest[..24]);
            Ok::<(), std::io::Error>(())
        })
        .map_err(|error| error.to_string())?;
    Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn standalone_window_label(owner: &str, identifier: &str, role: &str) -> String {
    let digest = Sha256::digest(format!("{owner}\0{identifier}\0{role}"));
    let suffix = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{STANDALONE_LABEL_PREFIX}{suffix}")
}

fn encode_uri_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn decode_uri_path(path: &str) -> Result<String, String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err("Plugin resource URL has invalid percent encoding".to_string());
        }
        let high = hex_value(bytes[index + 1])?;
        let low = hex_value(bytes[index + 2])?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(decoded).map_err(|_| "Plugin resource URL path must use UTF-8".to_string())
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("Plugin resource URL has invalid percent encoding".to_string()),
    }
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn inject_bridge(html: &str, token: &str, allowed_domains: &[String]) -> Vec<u8> {
    let script = bridge_script(token);
    let mut output = String::with_capacity(html.len() + script.len() + 256);
    if let Some(head) = find_ascii_case_insensitive(html.as_bytes(), b"<head")
        .and_then(|start| html[start..].find('>').map(|offset| start + offset + 1))
    {
        output.push_str(&html[..head]);
        output.push_str(&format!(
            "<meta http-equiv=\"Content-Security-Policy\" content=\"{}\">",
            html_attribute_escape(&content_security_policy(allowed_domains))
        ));
        output.push_str(&script);
        output.push_str(&html[head..]);
    } else {
        output.push_str(&format!(
            "<meta http-equiv=\"Content-Security-Policy\" content=\"{}\">",
            html_attribute_escape(&content_security_policy(allowed_domains))
        ));
        output.push_str(&script);
        output.push_str(html);
    }
    output.into_bytes()
}

fn bridge_script(token: &str) -> String {
    let token = serde_json::to_string(token).expect("plugin page token is serializable");
    format!(
        r#"<script>(() => {{
  "use strict";
  const token = {token};
  const endpoint = `{PLUGIN_WEBVIEW_SCHEME}://${{token}}.localhost/${{token}}/{BRIDGE_ENDPOINT}`;
  const listeners = Object.create(null);
  const deny = () => Promise.reject(new Error("Tauri IPC is unavailable in plugin pages"));
  try {{ if (window.__TAURI__?.core) window.__TAURI__.core.invoke = deny; }} catch (_) {{}}
  try {{ if (window.__TAURI_INTERNALS__) window.__TAURI_INTERNALS__.invoke = deny; }} catch (_) {{}}
  try {{ if (window.ipc) window.ipc.postMessage = deny; }} catch (_) {{}}
  try {{ Reflect.deleteProperty(window, "__TAURI__"); }} catch (_) {{}}
  try {{ Reflect.deleteProperty(window, "__TAURI_INTERNALS__"); }} catch (_) {{}}
  try {{ Reflect.deleteProperty(window, "ipc"); }} catch (_) {{}}
  const api = {{
    listeners,
    _emit(name, data) {{
      const callback = listeners[String(name)];
      if (typeof callback === "function") callback.call(null, data);
    }},
    _simpleModeSetStyle(style) {{
      const target = document.getElementById("style");
      if (target) target.textContent = String(style ?? "");
    }},
    _simpleModeSetContent(content) {{
      const target = document.getElementById("content");
      if (target) target.innerHTML = String(content ?? "");
    }},
    onMessage(name, callback) {{ listeners[String(name)] = callback; }},
    postMessage(name, data) {{
      const body = JSON.stringify({{ name: String(name), data }});
      if (window.parent !== window) {{
        window.parent.postMessage({{ __iimaPluginPagePost: true, token, ...JSON.parse(body) }}, "*");
        return;
      }}
      void fetch(endpoint, {{ method: "POST", body, credentials: "omit", cache: "no-store" }});
    }},
  }};
  const hitTest = (x, y) => {{
    const element = document.elementFromPoint(Number(x), Number(y));
    return Boolean(element && Object.prototype.hasOwnProperty.call(element.dataset || {{}}, "clickable"));
  }};
  const reportHitTest = (x, y, requestId = 0) => {{
    if (window.parent === window) return;
    window.parent.postMessage({{
      __iimaPluginPageHitTest: true,
      token,
      requestId,
      clickable: hitTest(x, y),
    }}, "*");
  }};
  window.iina = api;
  window.addEventListener("pointermove", (event) => reportHitTest(event.clientX, event.clientY), true);
  window.addEventListener("message", (event) => {{
    const message = event.data;
    if (!message || message.__iimaPluginBridge !== true || message.token !== token) return;
    if (message.control === "style") {{ api._simpleModeSetStyle(message.data); return; }}
    if (message.control === "content") {{ api._simpleModeSetContent(message.data); return; }}
    if (message.control === "hit-test") {{
      reportHitTest(message.x, message.y, Number(message.requestId) || 0);
      return;
    }}
    api._emit(message.name, message.data);
  }});
}})();</script>"#
    )
}

fn content_security_policy(allowed_domains: &[String]) -> String {
    let mut network = Vec::new();
    for domain in allowed_domains {
        let domain = domain.trim().to_ascii_lowercase();
        if domain == "*" {
            network = vec!["http:".to_string(), "https:".to_string()];
            break;
        }
        let valid = domain
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'*'));
        if !valid || domain.is_empty() || domain == "*." {
            continue;
        }
        network.push(format!("http://{domain}"));
        network.push(format!("https://{domain}"));
    }
    network.sort();
    network.dedup();
    let network = network.join(" ");
    let optional_network = if network.is_empty() {
        String::new()
    } else {
        format!(" {network}")
    };
    format!(
        "default-src 'self' {PLUGIN_WEBVIEW_SCHEME}: data: blob:{optional_network}; object-src 'none'; base-uri 'self'; frame-ancestors *; script-src 'self' {PLUGIN_WEBVIEW_SCHEME}: 'unsafe-inline' 'unsafe-eval'{optional_network}; style-src 'self' {PLUGIN_WEBVIEW_SCHEME}: 'unsafe-inline'{optional_network}; connect-src 'self' {PLUGIN_WEBVIEW_SCHEME}:{optional_network}"
    )
}

fn html_attribute_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn find_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}

fn simple_mode_html(surface: &str) -> &'static str {
    if surface == "overlay" {
        r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>Overlay</title>
<style>html,body{margin:0;width:100%;height:100%;background:transparent}body{font-size:13px;font-family:-apple-system,BlinkMacSystemFont,'Helvetica Neue',sans-serif;color:white;text-shadow:0 1px 0 black,0 -1px 0 black,-1px 0 0 black,1px 0 0 black}</style><style id="style"></style></head><body><div id="content"></div></body></html>"#
    } else {
        r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>Overlay</title>
<style>body{font-size:13px;font-family:-apple-system,BlinkMacSystemFont,'Helvetica Neue',sans-serif}@media(prefers-color-scheme:dark){body{color:#eee}body a{color:#007aff}}</style><style id="style"></style></head><body><div id="content"></div></body></html>"#
    }
}

#[cfg(target_os = "macos")]
fn configure_native_plugin_window(
    window: &WebviewWindow,
    full_size_content_view: Option<bool>,
    hide_title_bar: Option<bool>,
) -> Result<(), String> {
    use std::ffi::{c_int, c_void};
    unsafe extern "C" {
        fn iima_native_configure_plugin_window(
            window: *mut c_void,
            full_size_content_view: c_int,
            hide_title_bar: c_int,
        );
    }
    let pointer = window.ns_window().map_err(|error| error.to_string())?;
    unsafe {
        iima_native_configure_plugin_window(
            pointer,
            full_size_content_view.map(c_int::from).unwrap_or(-1),
            hide_title_bar.map(c_int::from).unwrap_or(-1),
        )
    };
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn configure_native_plugin_window(
    window: &WebviewWindow,
    full_size_content_view: Option<bool>,
    hide_title_bar: Option<bool>,
) -> Result<(), String> {
    if let Some(full_size) = full_size_content_view {
        window
            .set_title_bar_style(if full_size {
                tauri::TitleBarStyle::Overlay
            } else {
                tauri::TitleBarStyle::Visible
            })
            .map_err(|error| error.to_string())?;
    }
    if let Some(hidden) = hide_title_bar {
        window
            .set_decorations(!hidden)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_native_plugin_window_frame(
    window: &WebviewWindow,
    width: Option<f64>,
    height: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    use std::ffi::{c_int, c_void};
    unsafe extern "C" {
        fn iima_native_set_plugin_window_frame(
            window: *mut c_void,
            has_width: c_int,
            width: f64,
            has_height: c_int,
            height: f64,
            has_x: c_int,
            x: f64,
            has_y: c_int,
            y: f64,
        ) -> c_int;
    }
    let pointer = window.ns_window().map_err(|error| error.to_string())?;
    let status = unsafe {
        iima_native_set_plugin_window_frame(
            pointer,
            c_int::from(width.is_some()),
            width.unwrap_or_default(),
            c_int::from(height.is_some()),
            height.unwrap_or_default(),
            c_int::from(x.is_some()),
            x.unwrap_or_default(),
            c_int::from(y.is_some()),
            y.unwrap_or_default(),
        )
    };
    (status == 0)
        .then_some(())
        .ok_or_else(|| format!("Unable to set standalone plugin window frame ({status})"))
}

#[cfg(not(target_os = "macos"))]
fn set_native_plugin_window_frame(
    window: &WebviewWindow,
    width: Option<f64>,
    height: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    if width.is_some() || height.is_some() {
        let current = window.inner_size().map_err(|error| error.to_string())?;
        window
            .set_size(tauri::LogicalSize::new(
                width.unwrap_or(f64::from(current.width)),
                height.unwrap_or(f64::from(current.height)),
            ))
            .map_err(|error| error.to_string())?;
    }
    if x.is_some() || y.is_some() {
        let current = window.outer_position().map_err(|error| error.to_string())?;
        window
            .set_position(tauri::LogicalPosition::new(
                x.unwrap_or(f64::from(current.x)),
                y.unwrap_or(f64::from(current.y)),
            ))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_grant() -> PageGrant {
        PageGrant {
            identifier: "com.example.plugin".to_string(),
            role: "entry".to_string(),
            plugin_name: "Example".to_string(),
            owner_window_label: "main".to_string(),
            expected_webview_label: "main".to_string(),
            surface: "overlay".to_string(),
            root: std::env::temp_dir(),
            simple_mode: false,
            allowed_domains: Vec::new(),
            created_at: Instant::now(),
            resources: BTreeMap::new(),
            total_resource_bytes: 0,
        }
    }

    #[test]
    fn resource_paths_reject_absolute_and_parent_components() {
        assert_eq!(
            normalize_relative_path("ui/./index.html").unwrap(),
            "ui/index.html"
        );
        assert!(normalize_relative_path("../Info.json").is_err());
        assert!(normalize_relative_path("ui/../../Info.json").is_err());
        assert!(normalize_relative_path("/tmp/index.html").is_err());
        assert!(normalize_relative_path("").is_err());
    }

    #[test]
    fn canonical_resource_check_rejects_parent_and_nested_symlink_escape() {
        let base = std::env::temp_dir().join(format!(
            "iima-plugin-webview-path-{}-{}",
            std::process::id(),
            TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("plugin");
        fs::create_dir_all(root.join("ui")).unwrap();
        fs::write(root.join("ui/index.html"), "ok").unwrap();
        fs::write(base.join("outside.html"), "no").unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();
        assert!(secure_resource_path(&canonical_root, "ui/index.html").is_ok());
        assert!(secure_resource_path(&canonical_root, "../outside.html").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.join("outside.html"), root.join("ui/link.html"))
                .unwrap();
            assert!(secure_resource_path(&canonical_root, "ui/link.html").is_err());
        }
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn protocol_paths_decode_utf8_but_keep_authorization_segment_separate() {
        let token = "ab".repeat(24);
        let (decoded_token, path) =
            protocol_request_parts(&format!("/{token}/pages/%E6%B5%8B%E8%AF%95.html")).unwrap();
        assert_eq!(decoded_token, token);
        assert_eq!(path, "pages/测试.html");
        assert!(protocol_request_parts(&format!("/{token}/pages/%ZZ.html")).is_err());
        assert!(protocol_request_parts("/short/index.html").is_err());
    }

    #[test]
    fn injected_bridge_precedes_plugin_scripts_and_removes_tauri_globals() {
        let token = "cd".repeat(24);
        let output = String::from_utf8(inject_bridge(
            "<!doctype html><HTML><HEAD><script>plugin()</script></HEAD></HTML>",
            &token,
            &["api.example.com".to_string()],
        ))
        .unwrap();
        assert!(output.contains("window.iina = api"));
        assert!(output.contains("document.elementFromPoint"));
        assert!(output.contains("__iimaPluginPageHitTest"));
        assert!(output.contains("message.control === \"hit-test\""));
        assert!(output.contains("Reflect.deleteProperty(window, \"__TAURI__\")"));
        assert!(output.contains("iima-plugin://${token}.localhost/${token}/__iima_bridge__"));
        assert!(output.find("window.iina = api").unwrap() < output.find("plugin()").unwrap());
        assert!(output.contains("https://api.example.com"));
    }

    #[test]
    fn standalone_labels_are_bounded_and_owner_specific() {
        let first = standalone_window_label("main", "com.example.plugin", "entry");
        let second = standalone_window_label("player-2", "com.example.plugin", "entry");
        let global = standalone_window_label("main", "com.example.plugin", "global");
        assert!(first.starts_with(STANDALONE_LABEL_PREFIX));
        assert_ne!(first, second);
        assert_ne!(first, global);
        assert_eq!(first.len(), STANDALONE_LABEL_PREFIX.len() + 24);
    }

    #[test]
    fn webview_roles_reject_global_player_surfaces() {
        assert_eq!(validated_instance_role("entry").unwrap(), "entry");
        assert_eq!(validated_instance_role("global").unwrap(), "global");
        assert!(validated_instance_role("child").is_err());
    }

    #[test]
    fn content_types_cover_plugin_page_assets() {
        assert_eq!(
            content_type_for_path(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type_for_path(Path::new("main.mjs")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(content_type_for_path(Path::new("font.woff2")), "font/woff2");
        assert_eq!(content_type_for_path(Path::new("cover.webp")), "image/webp");
    }

    #[test]
    fn resource_accounting_caps_distinct_files_and_total_bytes_without_double_counting() {
        let mut grant = test_grant();
        account_resource_in_grant(&mut grant, "same.js", 1024).unwrap();
        account_resource_in_grant(&mut grant, "same.js", 1024).unwrap();
        assert_eq!(grant.resources.len(), 1);
        assert_eq!(grant.total_resource_bytes, 1024);
        for index in 1..MAX_RESOURCE_COUNT {
            account_resource_in_grant(&mut grant, &format!("asset-{index}"), 1).unwrap();
        }
        assert_eq!(grant.resources.len(), MAX_RESOURCE_COUNT);
        assert_eq!(
            account_resource_in_grant(&mut grant, "one-too-many", 1)
                .unwrap_err()
                .0,
            StatusCode::TOO_MANY_REQUESTS
        );

        let mut total = test_grant();
        account_resource_in_grant(&mut total, "large.bin", MAX_TOTAL_RESOURCE_BYTES).unwrap();
        assert_eq!(
            account_resource_in_grant(&mut total, "overflow.bin", 1)
                .unwrap_err()
                .0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}
