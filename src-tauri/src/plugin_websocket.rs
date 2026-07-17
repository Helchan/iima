use crate::{plugins, state::AppState};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, WebviewWindow};

pub const SERVER_STATE_EVENT: &str = "iima-plugin-websocket-state";
pub const NEW_CONNECTION_EVENT: &str = "iima-plugin-websocket-new-connection";
pub const CONNECTION_STATE_EVENT: &str = "iima-plugin-websocket-connection-state";
pub const MESSAGE_EVENT: &str = "iima-plugin-websocket-message";

const MAX_SERVERS: usize = 16;
const MAX_CONNECTIONS_PER_SERVER: usize = 32;
const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(200);
const LISTENER_POLL_INTERVAL: Duration = Duration::from_millis(20);
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

static SERVER_GENERATION: AtomicU64 = AtomicU64::new(1);
static CONNECTION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ServerKey {
    identifier: String,
    window_label: String,
    role: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerPhase {
    Setup,
    Running,
    Failed,
}

#[derive(Clone)]
struct ConnectionHandle {
    writer: Arc<Mutex<TcpStream>>,
    ready: Arc<AtomicBool>,
}

struct ServerEntry {
    generation: u64,
    port: u16,
    phase: ServerPhase,
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<HashMap<String, TcpStream>>>,
    connections: Arc<Mutex<HashMap<String, ConnectionHandle>>>,
    connection_threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
    listener_thread: Option<JoinHandle<()>>,
}

fn servers() -> &'static Mutex<HashMap<ServerKey, ServerEntry>> {
    static SERVERS: OnceLock<Mutex<HashMap<ServerKey, ServerEntry>>> = OnceLock::new();
    SERVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketServerCreated {
    pub generation: u64,
    pub port: u16,
    pub host: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebSocketErrorPayload {
    description: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerStatePayload {
    identifier: String,
    role: String,
    window_label: String,
    generation: u64,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<WebSocketErrorPayload>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewConnectionPayload {
    identifier: String,
    role: String,
    window_label: String,
    generation: u64,
    connection_id: String,
    path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionStatePayload {
    identifier: String,
    role: String,
    window_label: String,
    generation: u64,
    connection_id: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<WebSocketErrorPayload>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MessagePayload {
    identifier: String,
    role: String,
    window_label: String,
    generation: u64,
    connection_id: String,
    data: Vec<u8>,
}

fn error_payload(message: impl Into<String>) -> WebSocketErrorPayload {
    let message = message.into();
    WebSocketErrorPayload {
        description: message.clone(),
        message,
    }
}

fn ensure_plugin_runtime_is_enabled(
    app: &AppHandle,
    state: &AppState,
    identifier: &str,
) -> Result<(), String> {
    let plugin_system_enabled = state
        .preferences
        .lock()
        .map_err(|error| error.to_string())?
        .values
        .get("iinaEnablePluginSystem")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    if !plugin_system_enabled || !plugins::plugin_is_enabled(app, identifier)? {
        return Err("Plugin is not enabled".to_string());
    }
    Ok(())
}

fn validate_identifier(identifier: &str) -> Result<(), String> {
    if identifier.is_empty()
        || identifier.len() > 256
        || identifier.contains('\0')
        || !identifier.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
    {
        return Err("Plugin identifier is invalid".to_string());
    }
    Ok(())
}

fn validate_instance_role(role: &str) -> Result<(), String> {
    match role {
        "entry" | "global" => Ok(()),
        _ => Err("Plugin WebSocket role must be entry or global".to_string()),
    }
}

#[tauri::command]
pub fn plugin_websocket_create_server(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    role: String,
    port: u16,
) -> Result<WebSocketServerCreated, String> {
    validate_identifier(&identifier)?;
    validate_instance_role(&role)?;
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    if port == 0 {
        return Err("ws.createServer: port must be between 1 and 65535".to_string());
    }

    let key = ServerKey {
        identifier: identifier.clone(),
        window_label: window.label().to_string(),
        role,
    };
    let previous = {
        let mut registry = servers().lock().map_err(|error| error.to_string())?;
        registry.remove(&key)
    };
    if let Some(previous) = previous {
        stop_entry(&app, &key, previous, true);
    }

    let generation = SERVER_GENERATION.fetch_add(1, Ordering::Relaxed);
    let raced_replacement = {
        let mut registry = servers().lock().map_err(|error| error.to_string())?;
        if registry.len() >= MAX_SERVERS {
            return Err(format!(
                "ws.createServer: at most {MAX_SERVERS} plugin WebSocket servers may exist"
            ));
        }
        registry.insert(
            key.clone(),
            ServerEntry {
                generation,
                port,
                phase: ServerPhase::Setup,
                stop: Arc::new(AtomicBool::new(false)),
                sockets: Arc::new(Mutex::new(HashMap::new())),
                connections: Arc::new(Mutex::new(HashMap::new())),
                connection_threads: Arc::new(Mutex::new(Vec::new())),
                listener_thread: None,
            },
        )
    };
    if let Some(raced_replacement) = raced_replacement {
        stop_entry(&app, &key, raced_replacement, true);
    }
    emit_server_state(&app, &key, generation, "setup", None);
    Ok(WebSocketServerCreated {
        generation,
        port,
        host: "127.0.0.1",
    })
}

#[tauri::command]
pub fn plugin_websocket_start_server(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    role: String,
) -> Result<(), String> {
    validate_identifier(&identifier)?;
    validate_instance_role(&role)?;
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    let key = ServerKey {
        identifier,
        window_label: window.label().to_string(),
        role,
    };

    let listener = {
        let mut registry = servers().lock().map_err(|error| error.to_string())?;
        let entry = registry
            .get_mut(&key)
            .ok_or_else(|| "ws.startServer: server not created".to_string())?;
        if entry.phase != ServerPhase::Setup {
            return Err("ws.startServer: server is not in ready state".to_string());
        }
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, entry.port);
        match TcpListener::bind(address) {
            Ok(listener) => listener,
            Err(error) => {
                entry.phase = ServerPhase::Failed;
                let generation = entry.generation;
                drop(registry);
                emit_server_state(&app, &key, generation, "failed", Some(error.to_string()));
                return Err(format!(
                    "ws.startServer: unable to listen on {address}: {error}"
                ));
            }
        }
    };
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("ws.startServer: unable to configure listener: {error}"))?;

    let (generation, stop, sockets, connections, connection_threads) = {
        let registry = servers().lock().map_err(|error| error.to_string())?;
        let entry = registry
            .get(&key)
            .ok_or_else(|| "ws.startServer: server not created".to_string())?;
        (
            entry.generation,
            entry.stop.clone(),
            entry.sockets.clone(),
            entry.connections.clone(),
            entry.connection_threads.clone(),
        )
    };
    let worker_app = app.clone();
    let worker_key = key.clone();
    let worker_stop = stop.clone();
    let listener_thread = std::thread::Builder::new()
        .name(websocket_thread_name(&key.identifier, "listener"))
        .spawn(move || {
            run_listener(
                worker_app,
                worker_key,
                generation,
                listener,
                worker_stop,
                sockets,
                connections,
                connection_threads,
            );
        })
        .map_err(|error| format!("ws.startServer: unable to start listener: {error}"))?;

    {
        let mut registry = servers().lock().map_err(|error| error.to_string())?;
        let Some(entry) = registry.get_mut(&key) else {
            drop(registry);
            stop.store(true, Ordering::Release);
            let _ = listener_thread.join();
            return Err("ws.startServer: server was stopped while starting".to_string());
        };
        if entry.generation != generation || entry.phase != ServerPhase::Setup {
            drop(registry);
            stop.store(true, Ordering::Release);
            let _ = listener_thread.join();
            return Err("ws.startServer: server was replaced while starting".to_string());
        }
        entry.phase = ServerPhase::Running;
        entry.listener_thread = Some(listener_thread);
    }
    emit_server_state(&app, &key, generation, "ready", None);
    Ok(())
}

#[tauri::command]
pub fn plugin_websocket_send_text(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    role: String,
    connection_id: String,
    text: String,
) -> Result<String, String> {
    validate_identifier(&identifier)?;
    validate_instance_role(&role)?;
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    if connection_id.is_empty() || connection_id.len() > 128 || connection_id.contains('\0') {
        return Err("ws.sendText: connection identifier is invalid".to_string());
    }
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(format!(
            "ws.sendText: messages are limited to {MAX_MESSAGE_BYTES} bytes"
        ));
    }
    let key = ServerKey {
        identifier,
        window_label: window.label().to_string(),
        role,
    };
    let connection = {
        let registry = servers().lock().map_err(|error| error.to_string())?;
        let entry = registry
            .get(&key)
            .ok_or_else(|| "server does not exist".to_string())?;
        if entry.phase != ServerPhase::Running {
            return Err("server is not running".to_string());
        }
        let connection = entry
            .connections
            .lock()
            .map_err(|error| error.to_string())?
            .get(&connection_id)
            .cloned();
        connection
    };
    let Some(connection) = connection else {
        return Ok("no_connection".to_string());
    };
    if !connection.ready.load(Ordering::Acquire) {
        return Err("ws.sendText: connection is not ready".to_string());
    }

    // IINA 1.3.5's sendText implementation intentionally writes a binary frame.
    let mut writer = connection
        .writer
        .lock()
        .map_err(|error| error.to_string())?;
    write_server_frame(&mut *writer, 0x2, text.as_bytes())
        .map_err(|error| format!("ws.sendText: {error}"))?;
    Ok("success".to_string())
}

#[tauri::command]
pub fn plugin_websocket_stop(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    role: String,
) -> Result<bool, String> {
    validate_identifier(&identifier)?;
    validate_instance_role(&role)?;
    let key = ServerKey {
        identifier,
        window_label: window.label().to_string(),
        role,
    };
    let entry = servers()
        .lock()
        .map_err(|error| error.to_string())?
        .remove(&key);
    if let Some(entry) = entry {
        stop_entry(&app, &key, entry, true);
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn stop_identifier(app: &AppHandle, identifier: &str) {
    let entries = take_servers(|key| key.identifier == identifier);
    for (key, entry) in entries {
        stop_entry(app, &key, entry, true);
    }
}

pub fn stop_window(app: &AppHandle, window_label: &str) {
    let entries = take_servers(|key| key.window_label == window_label);
    for (key, entry) in entries {
        stop_entry(app, &key, entry, true);
    }
}

pub fn stop_all(app: &AppHandle) {
    let entries = take_servers(|_| true);
    for (key, entry) in entries {
        stop_entry(app, &key, entry, true);
    }
}

fn take_servers(predicate: impl Fn(&ServerKey) -> bool) -> Vec<(ServerKey, ServerEntry)> {
    let Ok(mut registry) = servers().lock() else {
        return Vec::new();
    };
    let keys = registry
        .keys()
        .filter(|key| predicate(key))
        .cloned()
        .collect::<Vec<_>>();
    keys.into_iter()
        .filter_map(|key| registry.remove(&key).map(|entry| (key, entry)))
        .collect()
}

fn stop_entry(app: &AppHandle, key: &ServerKey, mut entry: ServerEntry, emit_cancelled: bool) {
    entry.stop.store(true, Ordering::Release);
    if let Ok(mut sockets) = entry.sockets.lock() {
        for socket in sockets.values() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        sockets.clear();
    }
    if let Ok(mut connections) = entry.connections.lock() {
        for connection in connections.values() {
            if let Ok(writer) = connection.writer.lock() {
                let _ = writer.shutdown(Shutdown::Both);
            }
        }
        connections.clear();
    }
    if let Some(listener_thread) = entry.listener_thread.take() {
        let _ = listener_thread.join();
    }
    let connection_threads = entry
        .connection_threads
        .lock()
        .map(|mut threads| std::mem::take(&mut *threads))
        .unwrap_or_default();
    for connection_thread in connection_threads {
        let _ = connection_thread.join();
    }
    if emit_cancelled {
        emit_server_state(app, key, entry.generation, "cancelled", None);
    }
}

fn run_listener(
    app: AppHandle,
    key: ServerKey,
    generation: u64,
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<HashMap<String, TcpStream>>>,
    connections: Arc<Mutex<HashMap<String, ConnectionHandle>>>,
    connection_threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
) {
    let active_connections = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, remote)) => {
                reap_finished_connection_threads(&connection_threads);
                if active_connections.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS_PER_SERVER {
                    active_connections.fetch_sub(1, Ordering::AcqRel);
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                let connection_id = next_connection_id();
                let connection_app = app.clone();
                let connection_key = key.clone();
                let connection_stop = stop.clone();
                let connection_sockets = sockets.clone();
                let connection_map = connections.clone();
                let connection_count = active_connections.clone();
                let thread_name = websocket_thread_name(&key.identifier, "connection");
                match std::thread::Builder::new()
                    .name(thread_name)
                    .spawn(move || {
                        let _guard = ActiveConnectionGuard(connection_count);
                        handle_connection(
                            connection_app,
                            connection_key,
                            generation,
                            connection_id,
                            remote.to_string(),
                            stream,
                            connection_stop,
                            connection_sockets,
                            connection_map,
                        );
                    }) {
                    Ok(thread) => {
                        if let Ok(mut threads) = connection_threads.lock() {
                            threads.push(thread);
                        }
                    }
                    Err(error) => {
                        active_connections.fetch_sub(1, Ordering::AcqRel);
                        emit_server_state(
                            &app,
                            &key,
                            generation,
                            "failed",
                            Some(format!("Unable to create connection worker: {error}")),
                        );
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(LISTENER_POLL_INTERVAL);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                if !stop.load(Ordering::Acquire) {
                    emit_server_state(&app, &key, generation, "failed", Some(error.to_string()));
                }
                break;
            }
        }
    }
}

fn reap_finished_connection_threads(threads: &Arc<Mutex<Vec<JoinHandle<()>>>>) {
    let finished = {
        let Ok(mut threads) = threads.lock() else {
            return;
        };
        let all = std::mem::take(&mut *threads);
        let (finished, active): (Vec<_>, Vec<_>) =
            all.into_iter().partition(JoinHandle::is_finished);
        *threads = active;
        finished
    };
    for thread in finished {
        let _ = thread.join();
    }
}

struct ActiveConnectionGuard(Arc<AtomicUsize>);

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct SocketRegistration {
    connection_id: String,
    sockets: Arc<Mutex<HashMap<String, TcpStream>>>,
}

struct ConnectionRegistration {
    connection_id: String,
    connections: Arc<Mutex<HashMap<String, ConnectionHandle>>>,
}

impl Drop for ConnectionRegistration {
    fn drop(&mut self) {
        if let Ok(mut connections) = self.connections.lock() {
            connections.remove(&self.connection_id);
        }
    }
}

impl Drop for SocketRegistration {
    fn drop(&mut self) {
        if let Ok(mut sockets) = self.sockets.lock() {
            sockets.remove(&self.connection_id);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_connection(
    app: AppHandle,
    key: ServerKey,
    generation: u64,
    connection_id: String,
    remote_path: String,
    mut stream: TcpStream,
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<HashMap<String, TcpStream>>>,
    connections: Arc<Mutex<HashMap<String, ConnectionHandle>>>,
) {
    let _ = stream.set_read_timeout(Some(SOCKET_POLL_INTERVAL));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_nodelay(true);
    if stop.load(Ordering::Acquire) {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    let shutdown_socket = match stream.try_clone() {
        Ok(socket) => socket,
        Err(error) => {
            emit_connection_state(
                &app,
                &key,
                generation,
                &connection_id,
                "failed",
                Some(error.to_string()),
            );
            return;
        }
    };
    match sockets.lock() {
        Ok(mut sockets) => {
            sockets.insert(connection_id.clone(), shutdown_socket);
        }
        Err(error) => {
            emit_connection_state(
                &app,
                &key,
                generation,
                &connection_id,
                "failed",
                Some(error.to_string()),
            );
            return;
        }
    }
    let _socket_registration = SocketRegistration {
        connection_id: connection_id.clone(),
        sockets,
    };
    if stop.load(Ordering::Acquire) {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    let writer = match stream.try_clone() {
        Ok(writer) => Arc::new(Mutex::new(writer)),
        Err(error) => {
            emit_connection_state(
                &app,
                &key,
                generation,
                &connection_id,
                "failed",
                Some(error.to_string()),
            );
            return;
        }
    };
    let ready = Arc::new(AtomicBool::new(false));
    if let Ok(mut connection_map) = connections.lock() {
        connection_map.insert(
            connection_id.clone(),
            ConnectionHandle {
                writer: writer.clone(),
                ready: ready.clone(),
            },
        );
    } else {
        emit_connection_state(
            &app,
            &key,
            generation,
            &connection_id,
            "failed",
            Some("WebSocket connection registry is unavailable".to_string()),
        );
        return;
    }
    let _connection_registration = ConnectionRegistration {
        connection_id: connection_id.clone(),
        connections: connections.clone(),
    };
    emit_new_connection(&app, &key, generation, &connection_id, Some(remote_path));
    emit_connection_state(&app, &key, generation, &connection_id, "preparing", None);

    if let Err(error) = perform_server_handshake(&mut stream) {
        emit_connection_state(
            &app,
            &key,
            generation,
            &connection_id,
            "failed",
            Some(error),
        );
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    if stop.load(Ordering::Acquire) {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    ready.store(true, Ordering::Release);
    emit_connection_state(&app, &key, generation, &connection_id, "ready", None);

    let mut fragmented_message: Option<(u8, Vec<u8>)> = None;
    let mut failed = false;
    while !stop.load(Ordering::Acquire) {
        let frame = match read_client_frame(&mut stream, &stop) {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => {
                if !stop.load(Ordering::Acquire) {
                    let _ = write_close_frame(&writer, 1002);
                    emit_connection_state(
                        &app,
                        &key,
                        generation,
                        &connection_id,
                        "failed",
                        Some(error),
                    );
                    failed = true;
                }
                break;
            }
        };
        match frame.opcode {
            0x0 => {
                let Some((opcode, data)) = fragmented_message.as_mut() else {
                    let _ = write_close_frame(&writer, 1002);
                    failed = true;
                    emit_connection_state(
                        &app,
                        &key,
                        generation,
                        &connection_id,
                        "failed",
                        Some("Unexpected WebSocket continuation frame".to_string()),
                    );
                    break;
                };
                if data.len().saturating_add(frame.payload.len()) > MAX_MESSAGE_BYTES {
                    let _ = write_close_frame(&writer, 1009);
                    failed = true;
                    emit_connection_state(
                        &app,
                        &key,
                        generation,
                        &connection_id,
                        "failed",
                        Some("WebSocket message exceeds the resource limit".to_string()),
                    );
                    break;
                }
                data.extend_from_slice(&frame.payload);
                if frame.fin {
                    let opcode = *opcode;
                    let (_, message) = fragmented_message.take().expect("fragment exists");
                    if !deliver_message(
                        &app,
                        &key,
                        generation,
                        &connection_id,
                        opcode,
                        message,
                        &writer,
                    ) {
                        failed = true;
                        break;
                    }
                }
            }
            opcode @ (0x1 | 0x2) => {
                if fragmented_message.is_some() {
                    let _ = write_close_frame(&writer, 1002);
                    failed = true;
                    emit_connection_state(
                        &app,
                        &key,
                        generation,
                        &connection_id,
                        "failed",
                        Some(
                            "New WebSocket message started before continuation completed"
                                .to_string(),
                        ),
                    );
                    break;
                }
                if frame.fin {
                    if !deliver_message(
                        &app,
                        &key,
                        generation,
                        &connection_id,
                        opcode,
                        frame.payload,
                        &writer,
                    ) {
                        failed = true;
                        break;
                    }
                } else {
                    fragmented_message = Some((opcode, frame.payload));
                }
            }
            0x8 => {
                if let Err(error) = validate_close_payload(&frame.payload) {
                    let _ = write_close_frame(&writer, 1002);
                    failed = true;
                    emit_connection_state(
                        &app,
                        &key,
                        generation,
                        &connection_id,
                        "failed",
                        Some(error),
                    );
                } else if let Ok(mut output) = writer.lock() {
                    let _ = write_server_frame(&mut *output, 0x8, &frame.payload);
                }
                break;
            }
            0x9 => {
                if let Ok(mut output) = writer.lock() {
                    if write_server_frame(&mut *output, 0xA, &frame.payload).is_err() {
                        break;
                    }
                } else {
                    break;
                }
            }
            0xA => {}
            _ => {
                let _ = write_close_frame(&writer, 1002);
                failed = true;
                emit_connection_state(
                    &app,
                    &key,
                    generation,
                    &connection_id,
                    "failed",
                    Some("Unsupported WebSocket opcode".to_string()),
                );
                break;
            }
        }
    }

    let _ = stream.shutdown(Shutdown::Both);
    if !failed || stop.load(Ordering::Acquire) {
        emit_connection_state(&app, &key, generation, &connection_id, "cancelled", None);
    }
}

fn deliver_message(
    app: &AppHandle,
    key: &ServerKey,
    generation: u64,
    connection_id: &str,
    opcode: u8,
    message: Vec<u8>,
    writer: &Arc<Mutex<TcpStream>>,
) -> bool {
    if opcode == 0x1 && std::str::from_utf8(&message).is_err() {
        let _ = write_close_frame(writer, 1007);
        emit_connection_state(
            app,
            key,
            generation,
            connection_id,
            "failed",
            Some("WebSocket text message is not valid UTF-8".to_string()),
        );
        return false;
    }
    emit_message(app, key, generation, connection_id, message);
    true
}

fn perform_server_handshake(stream: &mut (impl Read + Write)) -> Result<(), String> {
    let request = read_http_headers(stream)?;
    let key = validate_handshake_request(&request)?;
    let accept = websocket_accept(&key);
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("Unable to write WebSocket handshake: {error}"))
}

fn read_http_headers(stream: &mut impl Read) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let mut request = Vec::with_capacity(1024);
    let mut byte = [0_u8; 1];
    while request.len() < MAX_HTTP_HEADER_BYTES {
        match stream.read(&mut byte) {
            Ok(0) => return Err("WebSocket client closed during handshake".to_string()),
            Ok(_) => {
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") {
                    return Ok(request);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    return Err("WebSocket handshake timed out".to_string());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("Unable to read WebSocket handshake: {error}")),
        }
    }
    Err(format!(
        "WebSocket handshake exceeds {MAX_HTTP_HEADER_BYTES} bytes"
    ))
}

fn validate_handshake_request(request: &[u8]) -> Result<String, String> {
    let request = std::str::from_utf8(request)
        .map_err(|_| "WebSocket handshake is not valid UTF-8".to_string())?;
    let mut lines = request.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "WebSocket handshake has no request line".to_string())?;
    let request_parts = request_line.split_ascii_whitespace().collect::<Vec<_>>();
    if request_parts.len() != 3
        || request_parts[0] != "GET"
        || !request_parts[1].starts_with('/')
        || request_parts[1].len() > 2048
        || request_parts[2] != "HTTP/1.1"
    {
        return Err("WebSocket handshake request line is invalid".to_string());
    }
    let mut headers: HashMap<String, String> = HashMap::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "WebSocket handshake contains an invalid header".to_string())?;
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("WebSocket handshake contains an invalid header name".to_string());
        }
        headers
            .entry(name)
            .and_modify(|existing| {
                existing.push(',');
                existing.push_str(value.trim());
            })
            .or_insert_with(|| value.trim().to_string());
    }
    let upgrade = headers
        .get("upgrade")
        .ok_or_else(|| "WebSocket handshake has no Upgrade header".to_string())?;
    if !upgrade.eq_ignore_ascii_case("websocket") {
        return Err("WebSocket Upgrade header is invalid".to_string());
    }
    let connection = headers
        .get("connection")
        .ok_or_else(|| "WebSocket handshake has no Connection header".to_string())?;
    if !connection
        .split(',')
        .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
    {
        return Err("WebSocket Connection header is invalid".to_string());
    }
    if headers.get("sec-websocket-version").map(String::as_str) != Some("13") {
        return Err("Only WebSocket version 13 is supported".to_string());
    }
    if !headers.get("host").is_some_and(|host| !host.is_empty()) {
        return Err("WebSocket handshake has no Host header".to_string());
    }
    let key = headers
        .get("sec-websocket-key")
        .ok_or_else(|| "WebSocket handshake has no key".to_string())?;
    let decoded = decode_base64(key).ok_or_else(|| "WebSocket key is invalid".to_string())?;
    if decoded.len() != 16 {
        return Err("WebSocket key must decode to 16 bytes".to_string());
    }
    Ok(key.clone())
}

fn websocket_accept(key: &str) -> String {
    let mut input = Vec::with_capacity(key.len() + WEBSOCKET_GUID.len());
    input.extend_from_slice(key.as_bytes());
    input.extend_from_slice(WEBSOCKET_GUID.as_bytes());
    encode_base64(&sha1(&input))
}

struct ClientFrame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

fn validate_close_payload(payload: &[u8]) -> Result<(), String> {
    if payload.len() == 1 {
        return Err("WebSocket close frame has a truncated status code".to_string());
    }
    if payload.len() < 2 {
        return Ok(());
    }
    let code = u16::from_be_bytes([payload[0], payload[1]]);
    let valid_code = matches!(code, 1000..=1003 | 1007..=1014 | 3000..=4999);
    if !valid_code {
        return Err("WebSocket close frame has an invalid status code".to_string());
    }
    std::str::from_utf8(&payload[2..])
        .map(|_| ())
        .map_err(|_| "WebSocket close reason is not valid UTF-8".to_string())
}

fn read_client_frame(
    stream: &mut impl Read,
    stop: &AtomicBool,
) -> Result<Option<ClientFrame>, String> {
    let mut header = [0_u8; 2];
    if !read_exact_polling(stream, &mut header, stop)? {
        return Ok(None);
    }
    let fin = header[0] & 0x80 != 0;
    let reserved = header[0] & 0x70;
    let opcode = header[0] & 0x0f;
    if reserved != 0 {
        return Err("WebSocket extensions are not enabled".to_string());
    }
    if header[1] & 0x80 == 0 {
        return Err("Client WebSocket frames must be masked".to_string());
    }
    let initial_length = (header[1] & 0x7f) as u64;
    let payload_length = match initial_length {
        126 => {
            let mut bytes = [0_u8; 2];
            read_required(stream, &mut bytes, stop)?;
            let length = u16::from_be_bytes(bytes) as u64;
            if length < 126 {
                return Err("WebSocket frame length is not canonically encoded".to_string());
            }
            length
        }
        127 => {
            let mut bytes = [0_u8; 8];
            read_required(stream, &mut bytes, stop)?;
            let length = u64::from_be_bytes(bytes);
            if length < 65_536 || length & (1_u64 << 63) != 0 {
                return Err("WebSocket frame length is not canonically encoded".to_string());
            }
            length
        }
        length => length,
    };
    let is_control = opcode & 0x08 != 0;
    if is_control && (!fin || payload_length > 125) {
        return Err("WebSocket control frame is invalid".to_string());
    }
    if payload_length > MAX_MESSAGE_BYTES as u64 {
        return Err(format!(
            "WebSocket frame exceeds the {MAX_MESSAGE_BYTES}-byte resource limit"
        ));
    }
    let mut mask = [0_u8; 4];
    read_required(stream, &mut mask, stop)?;
    let mut payload = vec![0_u8; payload_length as usize];
    read_required(stream, &mut payload, stop)?;
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % mask.len()];
    }
    Ok(Some(ClientFrame {
        fin,
        opcode,
        payload,
    }))
}

fn read_required(
    stream: &mut impl Read,
    output: &mut [u8],
    stop: &AtomicBool,
) -> Result<(), String> {
    if read_exact_polling(stream, output, stop)? {
        Ok(())
    } else {
        Err("WebSocket frame ended unexpectedly".to_string())
    }
}

fn read_exact_polling(
    stream: &mut impl Read,
    output: &mut [u8],
    stop: &AtomicBool,
) -> Result<bool, String> {
    let mut offset = 0;
    while offset < output.len() {
        if stop.load(Ordering::Acquire) {
            return Err("WebSocket server stopped".to_string());
        }
        match stream.read(&mut output[offset..]) {
            Ok(0) if offset == 0 => return Ok(false),
            Ok(0) => return Err("WebSocket frame ended unexpectedly".to_string()),
            Ok(count) => offset += count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("Unable to read WebSocket frame: {error}")),
        }
    }
    Ok(true)
}

fn write_server_frame(writer: &mut impl Write, opcode: u8, payload: &[u8]) -> Result<(), String> {
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err("WebSocket message exceeds the resource limit".to_string());
    }
    let mut header = Vec::with_capacity(10);
    header.push(0x80 | (opcode & 0x0f));
    match payload.len() {
        length @ 0..=125 => header.push(length as u8),
        length @ 126..=65_535 => {
            header.push(126);
            header.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            header.push(127);
            header.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    writer
        .write_all(&header)
        .and_then(|_| writer.write_all(payload))
        .map_err(|error| error.to_string())
}

fn write_close_frame(writer: &Arc<Mutex<TcpStream>>, code: u16) -> Result<(), String> {
    let mut writer = writer.lock().map_err(|error| error.to_string())?;
    write_server_frame(&mut *writer, 0x8, &code.to_be_bytes())
}

fn emit_server_state(
    app: &AppHandle,
    key: &ServerKey,
    generation: u64,
    state: &str,
    error: Option<String>,
) {
    let _ = app.emit_to(
        &key.window_label,
        SERVER_STATE_EVENT,
        ServerStatePayload {
            identifier: key.identifier.clone(),
            role: key.role.clone(),
            window_label: key.window_label.clone(),
            generation,
            state: state.to_string(),
            error: error.map(error_payload),
        },
    );
}

fn emit_new_connection(
    app: &AppHandle,
    key: &ServerKey,
    generation: u64,
    connection_id: &str,
    path: Option<String>,
) {
    let _ = app.emit_to(
        &key.window_label,
        NEW_CONNECTION_EVENT,
        NewConnectionPayload {
            identifier: key.identifier.clone(),
            role: key.role.clone(),
            window_label: key.window_label.clone(),
            generation,
            connection_id: connection_id.to_string(),
            path,
        },
    );
}

fn emit_connection_state(
    app: &AppHandle,
    key: &ServerKey,
    generation: u64,
    connection_id: &str,
    state: &str,
    error: Option<String>,
) {
    let _ = app.emit_to(
        &key.window_label,
        CONNECTION_STATE_EVENT,
        ConnectionStatePayload {
            identifier: key.identifier.clone(),
            role: key.role.clone(),
            window_label: key.window_label.clone(),
            generation,
            connection_id: connection_id.to_string(),
            state: state.to_string(),
            error: error.map(error_payload),
        },
    );
}

fn emit_message(
    app: &AppHandle,
    key: &ServerKey,
    generation: u64,
    connection_id: &str,
    data: Vec<u8>,
) {
    let _ = app.emit_to(
        &key.window_label,
        MESSAGE_EVENT,
        MessagePayload {
            identifier: key.identifier.clone(),
            role: key.role.clone(),
            window_label: key.window_label.clone(),
            generation,
            connection_id: connection_id.to_string(),
            data,
        },
    );
}

fn next_connection_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let sequence = CONNECTION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{timestamp:016x}-{sequence:016x}")
}

fn websocket_thread_name(identifier: &str, suffix: &str) -> String {
    let identifier = identifier.chars().take(32).collect::<String>();
    format!("iina-ws-{identifier}-{suffix}")
}

fn sha1(input: &[u8]) -> [u8; 20] {
    let mut message = input.to_vec();
    let bit_length = (message.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    let mut h0 = 0x6745_2301_u32;
    let mut h1 = 0xefcd_ab89_u32;
    let mut h2 = 0x98ba_dcfe_u32;
    let mut h3 = 0x1032_5476_u32;
    let mut h4 = 0xc3d2_e1f0_u32;
    for chunk in message.chunks_exact(64) {
        let mut words = [0_u32; 80];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (index, word) in words.iter().enumerate() {
            let (function, constant) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(function)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }
    let mut output = [0_u8; 20];
    for (index, word) in [h0, h1, h2, h3, h4].iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(third & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    if input.is_empty() || input.len() % 4 != 0 || !input.is_ascii() {
        return None;
    }
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    for (chunk_index, chunk) in bytes.chunks_exact(4).enumerate() {
        let is_last = chunk_index + 1 == bytes.len() / 4;
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            if !is_last || chunk[3] != b'=' {
                return None;
            }
            0
        } else {
            base64_value(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            if !is_last {
                return None;
            }
            0
        } else {
            base64_value(chunk[3])?
        };
        if chunk[2] == b'=' && b & 0x0f != 0 {
            return None;
        }
        if chunk[3] == b'=' && chunk[2] != b'=' && c & 0x03 != 0 {
            return None;
        }
        output.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            output.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            output.push((c << 6) | d);
        }
    }
    Some(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const VALID_REQUEST: &str = "GET /socket HTTP/1.1\r\nHost: 127.0.0.1:1234\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";

    #[test]
    fn server_keys_separate_entry_and_global_instances() {
        let entry = ServerKey {
            identifier: "io.iina.fixture".to_string(),
            window_label: "main".to_string(),
            role: "entry".to_string(),
        };
        let global = ServerKey {
            role: "global".to_string(),
            ..entry.clone()
        };
        assert_ne!(entry, global);
        assert!(validate_instance_role("entry").is_ok());
        assert!(validate_instance_role("global").is_ok());
        assert!(validate_instance_role("child").is_err());
    }

    #[test]
    fn computes_the_rfc6455_handshake_accept_value() {
        assert_eq!(
            websocket_accept("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        assert_eq!(
            validate_handshake_request(VALID_REQUEST.as_bytes()).unwrap(),
            "dGhlIHNhbXBsZSBub25jZQ=="
        );
    }

    #[test]
    fn rejects_non_websocket_and_oversized_handshakes() {
        assert!(validate_handshake_request(
            VALID_REQUEST
                .replace("Upgrade: websocket", "Upgrade: h2c")
                .as_bytes()
        )
        .is_err());
        assert!(validate_handshake_request(
            VALID_REQUEST
                .replace("Sec-WebSocket-Version: 13", "Sec-WebSocket-Version: 8")
                .as_bytes()
        )
        .is_err());
        assert!(validate_handshake_request(
            VALID_REQUEST
                .replace("Host: 127.0.0.1:1234\r\n", "")
                .as_bytes()
        )
        .is_err());
        assert!(validate_handshake_request(
            VALID_REQUEST
                .replace("dGhlIHNhbXBsZSBub25jZQ==", "c2hvcnQ=")
                .as_bytes()
        )
        .is_err());
        assert_eq!(MAX_HTTP_HEADER_BYTES, 16 * 1024);
        assert_eq!(MAX_MESSAGE_BYTES, 1024 * 1024);
        assert_eq!(MAX_CONNECTIONS_PER_SERVER, 32);
    }

    #[test]
    fn server_frames_are_unmasked_and_use_canonical_lengths() {
        let mut short = Vec::new();
        write_server_frame(&mut short, 0x2, b"hello").unwrap();
        assert_eq!(short, b"\x82\x05hello");

        let mut medium = Vec::new();
        write_server_frame(&mut medium, 0x2, &vec![7; 126]).unwrap();
        assert_eq!(&medium[..4], &[0x82, 126, 0, 126]);

        let mut large = Vec::new();
        write_server_frame(&mut large, 0x2, &vec![9; 65_536]).unwrap();
        assert_eq!(&large[..2], &[0x82, 127]);
        assert_eq!(u64::from_be_bytes(large[2..10].try_into().unwrap()), 65_536);
    }

    #[test]
    fn handshake_and_masked_message_interoperate_without_network_access() {
        let mut handshake = Cursor::new(VALID_REQUEST.as_bytes().to_vec());
        perform_server_handshake(&mut handshake).unwrap();
        let combined = handshake.into_inner();
        let response = String::from_utf8(combined[VALID_REQUEST.len()..].to_vec()).unwrap();
        assert!(response.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
        assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));

        let mask = [1_u8, 2, 3, 4];
        let payload = b"hello";
        let mut frame = vec![0x81, 0x80 | payload.len() as u8];
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        let stop = AtomicBool::new(false);
        let decoded = read_client_frame(&mut Cursor::new(frame), &stop)
            .unwrap()
            .unwrap();
        assert_eq!(decoded.opcode, 0x1);
        assert_eq!(decoded.payload, b"hello");
    }

    #[test]
    fn base64_validation_is_strict() {
        assert_eq!(decode_base64("dGhlIHNhbXBsZSBub25jZQ==").unwrap().len(), 16);
        assert!(decode_base64("dGhlIHNhbXBsZSBub25jZQ=").is_none());
        assert!(decode_base64("dGhlIHNhbXBsZSBub25jZQ=A").is_none());
        assert!(decode_base64("!!!!").is_none());
    }

    #[test]
    fn close_frames_require_valid_codes_and_utf8_reasons() {
        assert!(validate_close_payload(&[]).is_ok());
        assert!(validate_close_payload(&1000_u16.to_be_bytes()).is_ok());
        assert!(validate_close_payload(&[0x03]).is_err());
        assert!(validate_close_payload(&1005_u16.to_be_bytes()).is_err());
        let mut invalid_utf8 = 1000_u16.to_be_bytes().to_vec();
        invalid_utf8.push(0xff);
        assert!(validate_close_payload(&invalid_utf8).is_err());
    }
}
