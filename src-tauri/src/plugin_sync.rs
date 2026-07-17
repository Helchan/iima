use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::http::{header, Method, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::app_logging;
use crate::commands;
use crate::native_keychain;
use crate::plugin_utils;
use crate::plugin_websocket;
use crate::plugin_webview;
use crate::plugins;
use crate::state::AppState;

pub const PLUGIN_SYNC_SCHEME: &str = "iima-plugin-sync";
const PLUGIN_SYNC_ENDPOINT: &str = "iima-plugin-sync://localhost/invoke";
const PRODUCTION_ORIGIN: &str = "tauri://localhost";
const DEVELOPMENT_ORIGIN: &str = "http://127.0.0.1:1420";
const MAX_ACTIVE_GRANTS: usize = 128;
const GRANT_IDLE_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
const MAX_PROTOCOL_BYTES: usize = 64 * 1024 * 1024;
const MAX_JSON_DEPTH: usize = 32;
const MAX_JSON_OBJECT_KEYS: usize = 256;
const MAX_JSON_ARRAY_ITEMS: usize = 8 * 1024 * 1024;
const MAX_JSON_STRING_BYTES: usize = 8 * 1024 * 1024;
const MAX_JSON_NODES: usize = MAX_JSON_ARRAY_ITEMS + 4096;

static GRANTS: OnceLock<Mutex<BTreeMap<String, SyncGrant>>> = OnceLock::new();

#[derive(Debug, Clone)]
struct SyncGrant {
    identifier: String,
    role: String,
    owner_webview_label: String,
    last_used_at: Instant,
    file_handle_tokens: BTreeSet<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSyncGrant {
    token: String,
    endpoint: &'static str,
    role: String,
}

#[derive(Debug, Deserialize)]
struct SyncRequest {
    grant: String,
    method: String,
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize)]
struct SyncResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn grants() -> &'static Mutex<BTreeMap<String, SyncGrant>> {
    GRANTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn plugin_is_available(app: &AppHandle, state: &AppState, identifier: &str) -> Result<(), String> {
    let plugin_system_enabled = state
        .preferences
        .lock()
        .map_err(|error| error.to_string())?
        .values
        .get("iinaEnablePluginSystem")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !plugin_system_enabled || !plugins::plugin_is_enabled(app, identifier)? {
        return Err("Plugin is not enabled".to_string());
    }
    Ok(())
}

fn validate_instance_role(role: &str) -> Result<&str, String> {
    match role {
        "entry" | "global" => Ok(role),
        _ => Err("Plugin synchronization role must be entry or global".to_string()),
    }
}

fn new_grant_token() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| {
            format!("Secure plugin synchronization authorization is unavailable: {error}")
        })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn remove_expired_grants(entries: &mut BTreeMap<String, SyncGrant>) -> Vec<SyncGrant> {
    let expired = entries
        .iter()
        .filter(|(_, grant)| grant.last_used_at.elapsed() > GRANT_IDLE_LIFETIME)
        .map(|(token, _)| token.clone())
        .collect::<Vec<_>>();
    expired
        .into_iter()
        .filter_map(|token| entries.remove(&token))
        .collect()
}

fn cleanup_grant_file_handles(grants: &[SyncGrant]) -> usize {
    let tokens = grants
        .iter()
        .flat_map(|grant| grant.file_handle_tokens.iter().cloned())
        .collect::<Vec<_>>();
    let count = tokens.len();
    commands::cleanup_plugin_file_handle_tokens(&tokens);
    count
}

#[tauri::command]
pub fn plugin_sync_prepare_grant(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    role: String,
) -> Result<PluginSyncGrant, String> {
    plugin_is_available(&app, state.inner(), &identifier)?;
    validate_instance_role(&role)?;
    state.inner().player_session_for_window(window.label())?;
    let token = new_grant_token()?;
    let owner_webview_label = window.label().to_string();
    let (removed, inserted) = {
        let mut entries = grants().lock().map_err(|error| error.to_string())?;
        let mut removed = remove_expired_grants(&mut entries);
        let replaced = entries
            .iter()
            .filter(|(_, grant)| {
                grant.owner_webview_label == owner_webview_label
                    && grant.identifier == identifier
                    && grant.role == role
            })
            .map(|(token, _)| token.clone())
            .collect::<Vec<_>>();
        removed.extend(
            replaced
                .into_iter()
                .filter_map(|token| entries.remove(&token)),
        );
        let inserted = entries.len() < MAX_ACTIVE_GRANTS;
        if inserted {
            entries.insert(
                token.clone(),
                SyncGrant {
                    identifier,
                    role: role.clone(),
                    owner_webview_label,
                    last_used_at: Instant::now(),
                    file_handle_tokens: BTreeSet::new(),
                },
            );
        }
        (removed, inserted)
    };
    cleanup_grant_file_handles(&removed);
    if !inserted {
        return Err("Too many plugin synchronization authorizations are active".to_string());
    }
    Ok(PluginSyncGrant {
        token,
        endpoint: PLUGIN_SYNC_ENDPOINT,
        role,
    })
}

#[tauri::command]
pub fn plugin_sync_revoke_grant(
    window: WebviewWindow,
    identifier: String,
    grant: String,
) -> Result<(), String> {
    cleanup_grant(window.label(), &identifier, &grant);
    Ok(())
}

fn cleanup_grant(owner_webview_label: &str, identifier: &str, token: &str) {
    let removed = grants().lock().ok().and_then(|mut entries| {
        let matches = entries.get(token).is_some_and(|grant| {
            grant.owner_webview_label == owner_webview_label && grant.identifier == identifier
        });
        matches.then(|| entries.remove(token)).flatten()
    });
    if let Some(grant) = removed {
        cleanup_grant_file_handles(&[grant]);
    }
}

pub fn cleanup_owner(owner_webview_label: &str) {
    let removed = grants()
        .lock()
        .map(|mut entries| {
            let tokens = entries
                .iter()
                .filter(|(_, grant)| grant.owner_webview_label == owner_webview_label)
                .map(|(token, _)| token.clone())
                .collect::<Vec<_>>();
            tokens
                .into_iter()
                .filter_map(|token| entries.remove(&token))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let grant_count = removed.len();
    let handle_count = cleanup_grant_file_handles(&removed);
    commands::cleanup_plugin_file_handles_for_window(owner_webview_label);
    app_logging::log(
        "plugin-sync",
        0,
        format!(
            "owner cleanup completed for {owner_webview_label}: {grant_count} grant(s), {handle_count} tracked file handle(s)"
        ),
    );
}

pub fn cleanup_identifier(identifier: &str) {
    let removed = grants()
        .lock()
        .map(|mut entries| {
            let tokens = entries
                .iter()
                .filter(|(_, grant)| grant.identifier == identifier)
                .map(|(token, _)| token.clone())
                .collect::<Vec<_>>();
            tokens
                .into_iter()
                .filter_map(|token| entries.remove(&token))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let grant_count = removed.len();
    let handle_count = cleanup_grant_file_handles(&removed);
    commands::cleanup_plugin_file_handles_for_identifier(identifier);
    app_logging::log(
        "plugin-sync",
        0,
        format!(
            "identifier cleanup completed for {identifier}: {grant_count} grant(s), {handle_count} tracked file handle(s)"
        ),
    );
}

pub fn cleanup_all() {
    let removed = grants()
        .lock()
        .map(|mut entries| {
            std::mem::take(&mut *entries)
                .into_values()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let grant_count = removed.len();
    let handle_count = cleanup_grant_file_handles(&removed);
    commands::cleanup_all_plugin_file_handles();
    app_logging::log(
        "plugin-sync",
        0,
        format!(
            "application-exit cleanup completed: {grant_count} grant(s), {handle_count} tracked file handle(s)"
        ),
    );
}

pub fn handle_protocol(
    context: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    let allowed_origin = origin
        .filter(|origin| allowed_origin(origin))
        .map(str::to_string);
    let result = protocol_response(&context, &request);
    match result {
        Ok(ProtocolResponse::Preflight) => response(
            StatusCode::NO_CONTENT,
            allowed_origin.as_deref(),
            Vec::new(),
            true,
        ),
        Ok(ProtocolResponse::Invocation(bytes)) => {
            response(StatusCode::OK, allowed_origin.as_deref(), bytes, false)
        }
        Err((status, message)) => response(
            status,
            allowed_origin.as_deref(),
            serde_json::to_vec(&SyncResponse {
                ok: false,
                value: None,
                error: Some(message),
            })
            .unwrap_or_default(),
            false,
        ),
    }
}

enum ProtocolResponse {
    Preflight,
    Invocation(Vec<u8>),
}

fn protocol_response(
    context: &tauri::UriSchemeContext<'_, tauri::Wry>,
    request: &Request<Vec<u8>>,
) -> Result<ProtocolResponse, (StatusCode, String)> {
    if request.uri().host() != Some("localhost") || request.uri().path() != "/invoke" {
        return Err((
            StatusCode::NOT_FOUND,
            "Plugin synchronization endpoint is invalid".to_string(),
        ));
    }
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                "Plugin synchronization origin is missing".to_string(),
            )
        })?;
    if !allowed_origin(origin) {
        return Err((
            StatusCode::FORBIDDEN,
            "Plugin synchronization origin is not allowed".to_string(),
        ));
    }
    if request.method() == Method::OPTIONS {
        validate_preflight(request)?;
        app_logging::log(
            "plugin-sync",
            0,
            format!(
                "CORS preflight accepted for WebView {}",
                context.webview_label()
            ),
        );
        return Ok(ProtocolResponse::Preflight);
    }
    if request.method() != Method::POST {
        return Err((
            StatusCode::METHOD_NOT_ALLOWED,
            "Plugin synchronization accepts POST only".to_string(),
        ));
    }
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or_default().trim())
        .unwrap_or_default();
    if !content_type.eq_ignore_ascii_case("application/json") {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Plugin synchronization requires application/json".to_string(),
        ));
    }
    if request.body().len() > MAX_PROTOCOL_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "Plugin synchronization request exceeds 64 MiB".to_string(),
        ));
    }
    let payload: SyncRequest = serde_json::from_slice(request.body()).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Plugin synchronization request is invalid JSON".to_string(),
        )
    })?;
    validate_token(&payload.grant).map_err(|message| (StatusCode::FORBIDDEN, message))?;
    validate_method(&payload.method).map_err(|message| (StatusCode::BAD_REQUEST, message))?;
    validate_json(&payload.args).map_err(|message| (StatusCode::BAD_REQUEST, message))?;

    let grant = authorize_grant(&payload.grant, context.webview_label())
        .map_err(|message| (StatusCode::FORBIDDEN, message))?;
    let app = context.app_handle();
    let state = app.state::<AppState>();
    plugin_is_available(app, state.inner(), &grant.identifier)
        .map_err(|message| (StatusCode::FORBIDDEN, message))?;
    let window = app
        .get_webview_window(context.webview_label())
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                "Plugin synchronization owner WebView is unavailable".to_string(),
            )
        })?;
    let operation = dispatch(
        app,
        state,
        window,
        &payload.grant,
        &grant.identifier,
        &grant.role,
        &payload.method,
        payload.args,
    );
    let succeeded = operation.is_ok();
    let envelope = match operation {
        Ok(value) => {
            validate_json(&value)
                .map_err(|message| (StatusCode::INTERNAL_SERVER_ERROR, message))?;
            SyncResponse {
                ok: true,
                value: Some(value),
                error: None,
            }
        }
        Err(error) => SyncResponse {
            ok: false,
            value: None,
            error: Some(error),
        },
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unable to encode plugin synchronization response: {error}"),
        )
    })?;
    if bytes.len() > MAX_PROTOCOL_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "Plugin synchronization response exceeds 64 MiB".to_string(),
        ));
    }
    app_logging::log(
        "plugin-sync",
        0,
        format!(
            "synchronous invocation completed for {} {} ({}) in WebView {}: {}",
            grant.identifier,
            grant.role,
            payload.method,
            context.webview_label(),
            if succeeded { "ok" } else { "error" }
        ),
    );
    Ok(ProtocolResponse::Invocation(bytes))
}

fn allowed_origin(origin: &str) -> bool {
    origin == PRODUCTION_ORIGIN || (cfg!(debug_assertions) && origin == DEVELOPMENT_ORIGIN)
}

fn validate_preflight(request: &Request<Vec<u8>>) -> Result<(), (StatusCode, String)> {
    let requested_method = request
        .headers()
        .get("Access-Control-Request-Method")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !requested_method.eq_ignore_ascii_case("POST") {
        return Err((
            StatusCode::METHOD_NOT_ALLOWED,
            "Plugin synchronization preflight only permits POST".to_string(),
        ));
    }
    let requested_headers = request
        .headers()
        .get("Access-Control-Request-Headers")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if requested_headers
        .split(',')
        .map(str::trim)
        .any(|name| !name.is_empty() && !name.eq_ignore_ascii_case("content-type"))
    {
        return Err((
            StatusCode::FORBIDDEN,
            "Plugin synchronization preflight requested a forbidden header".to_string(),
        ));
    }
    Ok(())
}

fn response(
    status: StatusCode,
    origin: Option<&str>,
    body: Vec<u8>,
    preflight: bool,
) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .header(header::VARY, "Origin");
    if let Some(origin) = origin {
        builder = builder.header("Access-Control-Allow-Origin", origin);
    }
    if preflight {
        builder = builder
            .header("Access-Control-Allow-Methods", "POST")
            .header("Access-Control-Allow-Headers", "Content-Type")
            .header("Access-Control-Max-Age", "600");
    }
    builder
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn validate_token(token: &str) -> Result<(), String> {
    if token.len() != 64
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("Plugin synchronization authorization is invalid".to_string());
    }
    Ok(())
}

fn validate_method(method: &str) -> Result<(), String> {
    if method.is_empty()
        || method.len() > 96
        || !method
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'.')
    {
        return Err("Plugin synchronization method is invalid".to_string());
    }
    Ok(())
}

fn authorize_grant(token: &str, webview_label: &str) -> Result<SyncGrant, String> {
    let (expired, result) = {
        let mut entries = grants().lock().map_err(|error| error.to_string())?;
        let expired = remove_expired_grants(&mut entries);
        let result = entries
            .get_mut(token)
            .ok_or_else(|| "Plugin synchronization authorization expired".to_string())
            .and_then(|grant| {
                if grant.owner_webview_label != webview_label {
                    return Err(
                        "Plugin synchronization authorization belongs to another WebView"
                            .to_string(),
                    );
                }
                grant.last_used_at = Instant::now();
                Ok(grant.clone())
            });
        (expired, result)
    };
    cleanup_grant_file_handles(&expired);
    result
}

fn register_file_handle(
    grant_token: &str,
    webview_label: &str,
    identifier: &str,
    file_handle_token: &str,
) -> Result<(), String> {
    let mut entries = grants().lock().map_err(|error| error.to_string())?;
    let grant = entries
        .get_mut(grant_token)
        .ok_or_else(|| "Plugin synchronization authorization expired".to_string())?;
    if grant.owner_webview_label != webview_label || grant.identifier != identifier {
        return Err("Plugin synchronization authorization does not own this handle".to_string());
    }
    grant
        .file_handle_tokens
        .insert(file_handle_token.to_string());
    Ok(())
}

fn require_file_handle(grant_token: &str, file_handle_token: &str) -> Result<(), String> {
    let entries = grants().lock().map_err(|error| error.to_string())?;
    let grant = entries
        .get(grant_token)
        .ok_or_else(|| "Plugin synchronization authorization expired".to_string())?;
    if !grant.file_handle_tokens.contains(file_handle_token) {
        return Err("Plugin file handle belongs to another plugin instance".to_string());
    }
    Ok(())
}

fn unregister_file_handle(grant_token: &str, file_handle_token: &str) {
    if let Ok(mut entries) = grants().lock() {
        if let Some(grant) = entries.get_mut(grant_token) {
            grant.file_handle_tokens.remove(file_handle_token);
        }
    }
}

fn validate_json(value: &Value) -> Result<(), String> {
    fn walk(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), String> {
        if depth > MAX_JSON_DEPTH {
            return Err("Plugin synchronization JSON exceeds the depth limit".to_string());
        }
        *nodes = nodes.saturating_add(1);
        if *nodes > MAX_JSON_NODES {
            return Err("Plugin synchronization JSON contains too many values".to_string());
        }
        match value {
            Value::String(value) if value.len() > MAX_JSON_STRING_BYTES => {
                Err("Plugin synchronization JSON string exceeds 8 MiB".to_string())
            }
            Value::Array(values) => {
                if values.len() > MAX_JSON_ARRAY_ITEMS {
                    return Err(
                        "Plugin synchronization JSON array exceeds 8,388,608 items".to_string()
                    );
                }
                for value in values {
                    walk(value, depth + 1, nodes)?;
                }
                Ok(())
            }
            Value::Object(values) => {
                if values.len() > MAX_JSON_OBJECT_KEYS {
                    return Err(
                        "Plugin synchronization JSON object contains too many keys".to_string()
                    );
                }
                for (key, value) in values {
                    if key.len() > 256 {
                        return Err(
                            "Plugin synchronization JSON object key is too long".to_string()
                        );
                    }
                    walk(value, depth + 1, nodes)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    walk(value, 0, &mut 0)
}

fn args_object(args: Value) -> Result<Map<String, Value>, String> {
    match args {
        Value::Null => Ok(Map::new()),
        Value::Object(values) => Ok(values),
        _ => Err("Plugin synchronization arguments must be an object".to_string()),
    }
}

fn take_string(args: &mut Map<String, Value>, key: &str) -> Result<String, String> {
    args.remove(key)
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| format!("Plugin synchronization argument {key} must be a string"))
}

fn take_optional_string(
    args: &mut Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match args.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(format!(
            "Plugin synchronization argument {key} must be a string or null"
        )),
    }
}

fn take_bool(args: &mut Map<String, Value>, key: &str, default: bool) -> Result<bool, String> {
    match args.remove(key) {
        None => Ok(default),
        Some(Value::Bool(value)) => Ok(value),
        Some(_) => Err(format!(
            "Plugin synchronization argument {key} must be a boolean"
        )),
    }
}

fn take_u64(args: &mut Map<String, Value>, key: &str) -> Result<u64, String> {
    args.remove(key)
        .and_then(|value| value.as_u64())
        .ok_or_else(|| format!("Plugin synchronization argument {key} must be an unsigned integer"))
}

fn take_string_array(args: &mut Map<String, Value>, key: &str) -> Result<Vec<String>, String> {
    let values = args
        .remove(key)
        .and_then(|value| value.as_array().cloned())
        .ok_or_else(|| format!("Plugin synchronization argument {key} must be an array"))?;
    values
        .into_iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                format!("Plugin synchronization argument {key} must contain only strings")
            })
        })
        .collect()
}

fn json<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

fn silent_plugin_path_error(error: &str) -> bool {
    error == "@current is unavailable without a local media file"
        || error == "Current media path has no parent directory"
}

fn sync_file_path_resolves(
    app: &AppHandle,
    state: &AppState,
    window: &WebviewWindow,
    identifier: &str,
    path: &str,
) -> Result<bool, String> {
    let session = state.player_session_for_window(window.label())?;
    match commands::plugin_file_path_for_command(app, state, &session, identifier, path) {
        Ok(_) => Ok(true),
        Err(error) if silent_plugin_path_error(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

fn dispatch(
    app: &AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    grant_token: &str,
    identifier: &str,
    role: &str,
    method: &str,
    args: Value,
) -> Result<Value, String> {
    let mut args = args_object(args)?;
    let identifier = identifier.to_string();
    match method {
        "core.resolveopen" => json(commands::plugin_core_resolve_open_path(
            app,
            state.inner(),
            &window,
            &identifier,
            &take_string(&mut args, "path")?,
        )?),
        "core.version" => Ok(serde_json::json!({
            "iina": crate::about_window::IINA_VERSION,
            "build": crate::about_window::IINA_BUILD,
        })),
        "core.history" => json(state.inner().playback_history()?),
        "core.window.snapshot" => {
            let frame = crate::window_size::current_player_window_frame(&window)?;
            let primary = window
                .primary_monitor()
                .map_err(|error| error.to_string())?;
            let current = window
                .current_monitor()
                .map_err(|error| error.to_string())?;
            let primary_height = primary
                .as_ref()
                .map(|monitor| f64::from(monitor.size().height) / monitor.scale_factor())
                .unwrap_or_default();
            let same_monitor = |left: &tauri::Monitor, right: Option<&tauri::Monitor>| {
                right.is_some_and(|right| {
                    left.name() == right.name()
                        && left.position() == right.position()
                        && left.size() == right.size()
                })
            };
            let screens = window
                .available_monitors()
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|monitor| {
                    let scale = monitor.scale_factor().max(f64::EPSILON);
                    let width = f64::from(monitor.size().width) / scale;
                    let height = f64::from(monitor.size().height) / scale;
                    let x = f64::from(monitor.position().x) / scale;
                    let top = f64::from(monitor.position().y) / scale;
                    serde_json::json!({
                        "frame": {
                            "x": x,
                            "y": primary_height - top - height,
                            "width": width,
                            "height": height,
                        },
                        "main": same_monitor(&monitor, primary.as_ref()),
                        "current": same_monitor(&monitor, current.as_ref()),
                    })
                })
                .collect::<Vec<_>>();
            Ok(serde_json::json!({
                "loaded": true,
                "frame": frame,
                "fullscreen": commands::player_window_is_fullscreen(&window)?,
                "ontop": window.is_always_on_top().map_err(|error| error.to_string())?,
                "visible": window.is_visible().map_err(|error| error.to_string())?,
                "screens": screens,
            }))
        }
        "core.window.setframe" => {
            let frame = args
                .remove("frame")
                .and_then(|value| value.as_object().cloned())
                .ok_or_else(|| "Plugin synchronization window frame is invalid".to_string())?;
            let number = |key: &str| {
                frame
                    .get(key)
                    .and_then(Value::as_f64)
                    .ok_or_else(|| "Plugin synchronization window frame is invalid".to_string())
            };
            crate::window_size::set_player_window_frame(
                &window,
                crate::window_size::WindowFrame {
                    x: number("x")?,
                    y: number("y")?,
                    width: number("width")?,
                    height: number("height")?,
                },
            )?;
            Ok(Value::Null)
        }
        "mpv.get" => {
            let property = take_string(&mut args, "property")?;
            let kind = serde_json::from_value::<crate::mpv::MpvPluginGetKind>(Value::String(
                take_string(&mut args, "kind")?,
            ))
            .map_err(|_| "Plugin synchronization mpv get kind is invalid".to_string())?;
            json(commands::plugin_mpv_get_sync(
                app,
                state.inner(),
                &window,
                &identifier,
                &property,
                kind,
            )?)
        }
        "mpv.set" => {
            let property = take_string(&mut args, "property")?;
            let value = args
                .remove("value")
                .ok_or_else(|| "Plugin synchronization argument value is required".to_string())?;
            let value = serde_json::from_value::<crate::mpv::MpvPluginValue>(value)
                .map_err(|_| "Plugin synchronization mpv value is invalid".to_string())?;
            commands::plugin_mpv_set_sync(
                app,
                state.inner(),
                &window,
                &identifier,
                property,
                value,
            )?;
            Ok(Value::Null)
        }
        "mpv.command" => {
            let command = take_string(&mut args, "command")?;
            let command_args = take_string_array(&mut args, "args")?;
            commands::plugin_mpv_command_sync(
                app,
                state.inner(),
                &window,
                &identifier,
                command,
                command_args,
            )?;
            Ok(Value::Null)
        }
        "file.exists" => {
            let path = take_string(&mut args, "path")?;
            if !sync_file_path_resolves(app, state.inner(), &window, &identifier, &path)? {
                return Ok(Value::Bool(false));
            }
            json(commands::plugin_file_exists(
                app.clone(),
                state,
                window,
                identifier,
                path,
            )?)
        }
        "file.list" => {
            let path = take_string(&mut args, "path")?;
            let include_sub_dir = take_bool(&mut args, "includeSubDir", false)?;
            if !sync_file_path_resolves(app, state.inner(), &window, &identifier, &path)? {
                return Ok(Value::Null);
            }
            match commands::plugin_file_list(
                app.clone(),
                state,
                window,
                identifier,
                path,
                Some(include_sub_dir),
            ) {
                Ok(entries) => json(entries),
                Err(error) if error != "Plugin file listing exceeds 1000 entries" => {
                    Ok(Value::Null)
                }
                Err(error) => Err(error),
            }
        }
        "file.read" => {
            let path = take_string(&mut args, "path")?;
            let encoding = take_optional_string(&mut args, "encoding")?;
            if !sync_file_path_resolves(app, state.inner(), &window, &identifier, &path)? {
                return Ok(Value::Null);
            }
            json(commands::plugin_file_read(
                app.clone(),
                state,
                window,
                identifier,
                path,
                encoding,
            )?)
        }
        "file.write" => {
            let path = take_string(&mut args, "path")?;
            let content = take_string(&mut args, "content")?;
            if !sync_file_path_resolves(app, state.inner(), &window, &identifier, &path)? {
                return Ok(Value::Null);
            }
            commands::plugin_file_write(app.clone(), state, window, identifier, path, content)?;
            Ok(Value::Null)
        }
        "file.trash" => {
            let path = take_string(&mut args, "path")?;
            if !sync_file_path_resolves(app, state.inner(), &window, &identifier, &path)? {
                return Ok(Value::Null);
            }
            commands::plugin_file_trash(app.clone(), state, window, identifier, path)?;
            Ok(Value::Null)
        }
        "file.delete" => {
            let path = take_string(&mut args, "path")?;
            if !sync_file_path_resolves(app, state.inner(), &window, &identifier, &path)? {
                return Ok(Value::Null);
            }
            commands::plugin_file_delete(app.clone(), state, window, identifier, path)?;
            Ok(Value::Null)
        }
        "file.showinfinder" => {
            let path = take_string(&mut args, "path")?;
            if !sync_file_path_resolves(app, state.inner(), &window, &identifier, &path)? {
                return Ok(Value::Null);
            }
            commands::plugin_file_show_in_finder(app.clone(), state, window, identifier, path)?;
            Ok(Value::Null)
        }
        "file.handle.open" => {
            let path = take_string(&mut args, "path")?;
            let mode = take_string(&mut args, "mode")?;
            if !sync_file_path_resolves(app, state.inner(), &window, &identifier, &path)? {
                return Ok(Value::Null);
            }
            let file_handle_token = commands::plugin_file_handle_open(
                app.clone(),
                state,
                window.clone(),
                identifier.clone(),
                path,
                mode,
            )?;
            if let Err(error) =
                register_file_handle(grant_token, window.label(), &identifier, &file_handle_token)
            {
                let _ = commands::plugin_file_handle_close(
                    window,
                    identifier,
                    file_handle_token.clone(),
                );
                return Err(error);
            }
            json(file_handle_token)
        }
        "file.handle.offset" => {
            let token = take_string(&mut args, "token")?;
            require_file_handle(grant_token, &token)?;
            json(commands::plugin_file_handle_offset(
                window, identifier, token,
            )?)
        }
        "file.handle.seek" => {
            let token = take_string(&mut args, "token")?;
            require_file_handle(grant_token, &token)?;
            let offset = take_u64(&mut args, "offset")?;
            commands::plugin_file_handle_seek(window, identifier, token, offset)?;
            Ok(Value::Null)
        }
        "file.handle.seektoend" => {
            let token = take_string(&mut args, "token")?;
            require_file_handle(grant_token, &token)?;
            commands::plugin_file_handle_seek_to_end(window, identifier, token)?;
            Ok(Value::Null)
        }
        "file.handle.read" => {
            let token = take_string(&mut args, "token")?;
            require_file_handle(grant_token, &token)?;
            let length = usize::try_from(take_u64(&mut args, "length")?)
                .map_err(|_| "Plugin file handle read length is invalid".to_string())?;
            match commands::plugin_file_handle_read(window, identifier, token, length) {
                Ok(bytes) => json(bytes),
                Err(error) if error == "Plugin file handle is not open for reading" => {
                    Ok(Value::Null)
                }
                Err(error) => Err(error),
            }
        }
        "file.handle.readtoend" => {
            let token = take_string(&mut args, "token")?;
            require_file_handle(grant_token, &token)?;
            match commands::plugin_file_handle_read_to_end(window, identifier, token) {
                Ok(bytes) => json(bytes),
                Err(error) if error == "Plugin file handle is not open for reading" => {
                    Ok(Value::Null)
                }
                Err(error) => Err(error),
            }
        }
        "file.handle.write" => {
            let token = take_string(&mut args, "token")?;
            require_file_handle(grant_token, &token)?;
            let data = args
                .remove("data")
                .ok_or_else(|| "Plugin synchronization argument data is required".to_string())?;
            commands::plugin_file_handle_write(window, identifier, token, data)?;
            Ok(Value::Null)
        }
        "file.handle.close" => {
            let token = take_string(&mut args, "token")?;
            require_file_handle(grant_token, &token)?;
            commands::plugin_file_handle_close(window, identifier, token.clone())?;
            unregister_file_handle(grant_token, &token);
            Ok(Value::Null)
        }
        "utils.fileinpath" => json(plugin_utils::plugin_utils_file_in_path(
            app.clone(),
            state,
            window,
            identifier,
            take_string(&mut args, "file")?,
        )?),
        "utils.resolvepath" => json(plugin_utils::plugin_utils_resolve_path(
            app.clone(),
            state,
            window,
            identifier,
            take_string(&mut args, "path")?,
        )?),
        "utils.ask" => json(plugin_utils::plugin_utils_ask(
            app.clone(),
            identifier,
            take_string(&mut args, "title")?,
        )?),
        "utils.prompt" => json(plugin_utils::plugin_utils_prompt(
            app.clone(),
            identifier,
            take_string(&mut args, "title")?,
        )?),
        "utils.keychainread" => {
            let service = take_string(&mut args, "service")?;
            let name = take_string(&mut args, "name")?;
            json(plugin_keychain_read(
                app,
                state.inner(),
                &identifier,
                &service,
                &name,
            )?)
        }
        "utils.keychainwrite" => {
            let service = take_string(&mut args, "service")?;
            let name = take_string(&mut args, "name")?;
            let password = take_string(&mut args, "password")?;
            plugin_keychain_write(app, state.inner(), &identifier, &service, &name, &password)?;
            Ok(Value::Bool(true))
        }
        "utils.open" => json(plugin_utils::plugin_utils_open(
            app.clone(),
            state,
            window,
            identifier,
            take_string(&mut args, "url")?,
        )?),
        "standalone.isopen" => json(plugin_webview::plugin_standalone_window_is_open(
            app.clone(),
            window,
            identifier,
            role.to_string(),
        )?),
        "ws.createserver" => {
            let port = u16::try_from(take_u64(&mut args, "port")?)
                .map_err(|_| "ws.createServer: port not specified".to_string())?;
            json(plugin_websocket::plugin_websocket_create_server(
                app.clone(),
                state,
                window,
                identifier,
                role.to_string(),
                port,
            )?)
        }
        "ws.startserver" => {
            plugin_websocket::plugin_websocket_start_server(
                app.clone(),
                state,
                window,
                identifier,
                role.to_string(),
            )?;
            Ok(Value::Null)
        }
        _ => Err(format!("Unknown plugin synchronization method {method}")),
    }
}

fn plugin_keychain_service(identifier: &str, service: &str) -> Result<String, String> {
    if service.is_empty() || service.len() > 256 || identifier.len() > 256 || service.contains('\0')
    {
        return Err("Plugin Keychain service is invalid".to_string());
    }
    Ok(format!("{identifier} - {service}"))
}

fn validate_keychain_account(name: &str) -> Result<(), String> {
    if name.len() > 1024 || name.contains('\0') {
        Err("Plugin Keychain account is invalid".to_string())
    } else {
        Ok(())
    }
}

fn plugin_keychain_read(
    app: &AppHandle,
    state: &AppState,
    identifier: &str,
    service: &str,
    name: &str,
) -> Result<Option<String>, String> {
    plugin_is_available(app, state, identifier)?;
    validate_keychain_account(name)?;
    native_keychain::read_generic_password(&plugin_keychain_service(identifier, service)?, name)
}

fn plugin_keychain_write(
    app: &AppHandle,
    state: &AppState,
    identifier: &str,
    service: &str,
    name: &str,
    password: &str,
) -> Result<(), String> {
    plugin_is_available(app, state, identifier)?;
    validate_keychain_account(name)?;
    if password.len() > 64 * 1024 || password.contains('\0') {
        return Err("Plugin Keychain account or password is invalid".to_string());
    }
    native_keychain::write_generic_password(
        &plugin_keychain_service(identifier, service)?,
        name,
        password,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        allowed_origin, remove_expired_grants, response, silent_plugin_path_error,
        validate_instance_role, validate_json, validate_method, validate_preflight, validate_token,
        SyncGrant, GRANT_IDLE_LIFETIME, MAX_JSON_ARRAY_ITEMS, MAX_JSON_DEPTH,
    };
    use serde_json::json;
    use serde_json::Value;
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::Instant;
    use tauri::http::{Method, Request, StatusCode};

    #[test]
    fn token_origin_and_method_validation_fail_closed() {
        assert!(validate_token(&"a".repeat(64)).is_ok());
        assert!(validate_token(&"A".repeat(64)).is_err());
        assert!(validate_token(&"a".repeat(63)).is_err());
        assert!(allowed_origin("tauri://localhost"));
        assert!(allowed_origin("http://127.0.0.1:1420"));
        assert!(!allowed_origin("http://localhost:1420"));
        assert!(!allowed_origin("null"));
        assert!(validate_method("file.handle.readtoend").is_ok());
        assert!(validate_method("file/handle/read").is_err());
    }

    #[test]
    fn json_shape_is_bounded_without_shrinking_handle_bytes() {
        assert!(validate_json(&json!({"data": [0, 127, 255]})).is_ok());
        let mut deep = Value::Null;
        for _ in 0..=MAX_JSON_DEPTH {
            deep = Value::Array(vec![deep]);
        }
        assert!(validate_json(&deep).is_err());
        let oversized = Value::Array(vec![Value::Null; MAX_JSON_ARRAY_ITEMS + 1]);
        assert!(validate_json(&oversized).is_err());
    }

    #[test]
    fn only_unavailable_current_paths_use_file_api_guard_returns() {
        assert!(silent_plugin_path_error(
            "@current is unavailable without a local media file"
        ));
        assert!(silent_plugin_path_error(
            "Current media path has no parent directory"
        ));
        assert!(!silent_plugin_path_error(
            "To call this API, the plugin must declare permission \"file-system\" in its Info.json."
        ));
        assert!(!silent_plugin_path_error(
            "The path should be an absolute path: relative/file"
        ));
        assert!(!silent_plugin_path_error(
            "Cannot find the file path of track @sub/8. Perhaps it's an internal stream?"
        ));
    }

    #[test]
    fn cors_response_reflects_only_a_prevalidated_origin() {
        let response = response(
            StatusCode::OK,
            Some("tauri://localhost"),
            br#"{"ok":true}"#.to_vec(),
            false,
        );
        assert_eq!(
            response.headers()["Access-Control-Allow-Origin"],
            "tauri://localhost"
        );
        assert_eq!(response.headers()["Vary"], "Origin");
        assert_eq!(response.headers()["Cache-Control"], "no-store");
        assert_eq!(response.headers()["X-Content-Type-Options"], "nosniff");
    }

    #[test]
    fn cors_preflight_is_narrow_and_never_needs_a_grant() {
        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("iima-plugin-sync://localhost/invoke")
            .header("Origin", "tauri://localhost")
            .header("Access-Control-Request-Method", "POST")
            .header("Access-Control-Request-Headers", "content-type")
            .body(Vec::new())
            .unwrap();
        assert!(validate_preflight(&request).is_ok());
        let response = response(
            StatusCode::NO_CONTENT,
            Some("tauri://localhost"),
            Vec::new(),
            true,
        );
        assert_eq!(response.headers()["Access-Control-Allow-Methods"], "POST");
        assert_eq!(
            response.headers()["Access-Control-Allow-Headers"],
            "Content-Type"
        );

        let forbidden = Request::builder()
            .method(Method::OPTIONS)
            .uri("iima-plugin-sync://localhost/invoke")
            .header("Access-Control-Request-Method", "POST")
            .header("Access-Control-Request-Headers", "authorization")
            .body(Vec::new())
            .unwrap();
        assert!(validate_preflight(&forbidden).is_err());
    }

    #[test]
    fn expired_grants_return_their_exact_file_handles_for_cleanup_after_unlock() {
        let mut entries = BTreeMap::from([
            (
                "expired".to_string(),
                SyncGrant {
                    identifier: "io.iina.expired".to_string(),
                    role: "entry".to_string(),
                    owner_webview_label: "main".to_string(),
                    last_used_at: Instant::now()
                        - GRANT_IDLE_LIFETIME
                        - std::time::Duration::from_secs(1),
                    file_handle_tokens: BTreeSet::from(["old-handle".to_string()]),
                },
            ),
            (
                "live".to_string(),
                SyncGrant {
                    identifier: "io.iina.live".to_string(),
                    role: "global".to_string(),
                    owner_webview_label: "main".to_string(),
                    last_used_at: Instant::now(),
                    file_handle_tokens: BTreeSet::from(["live-handle".to_string()]),
                },
            ),
        ]);

        let expired = remove_expired_grants(&mut entries);
        assert_eq!(expired.len(), 1);
        assert!(expired[0].file_handle_tokens.contains("old-handle"));
        assert!(entries.contains_key("live"));
        assert!(!entries.contains_key("expired"));
    }

    #[test]
    fn synchronization_roles_are_explicit_instance_boundaries() {
        assert_eq!(validate_instance_role("entry").unwrap(), "entry");
        assert_eq!(validate_instance_role("global").unwrap(), "global");
        assert!(validate_instance_role("child").is_err());
        assert!(validate_instance_role("").is_err());
    }
}
