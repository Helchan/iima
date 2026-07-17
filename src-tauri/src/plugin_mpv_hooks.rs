use crate::{plugins, state::AppState};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, WebviewWindow};

pub const HOOK_EVENT: &str = "iima-plugin-mpv-hook";

const MAX_HOOK_NAME_BYTES: usize = 256;
const MAX_ACTIVE_HOOKS_PER_PLUGIN_WINDOW: usize = 128;
const MAX_LIFETIME_HOOKS_PER_WINDOW: usize = 4_096;
const FIRST_PLUGIN_HOOK_USERDATA: u64 = 2_000_000;

static NEXT_HOOK_USERDATA: AtomicU64 = AtomicU64::new(FIRST_PLUGIN_HOOK_USERDATA);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RegistrationKey {
    window_label: String,
    identifier: String,
    callback_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HookRegistration {
    key: RegistrationKey,
    reply_userdata: u64,
    active: bool,
}

#[derive(Default)]
struct HookRegistry {
    by_userdata: HashMap<(String, u64), HookRegistration>,
    by_callback: HashMap<RegistrationKey, u64>,
    pending: HashMap<(String, u64), u64>,
    known_windows: HashSet<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HookEventPayload {
    identifier: String,
    window_label: String,
    callback_id: u64,
    hook_id: u64,
    name: String,
}

fn registry() -> &'static Mutex<HookRegistry> {
    static REGISTRY: OnceLock<Mutex<HookRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HookRegistry::default()))
}

fn validate_hook_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.as_bytes().len() > MAX_HOOK_NAME_BYTES || name.contains('\0') {
        return Err(format!(
            "mpv hook names must be 1-{MAX_HOOK_NAME_BYTES} bytes and cannot contain NUL"
        ));
    }
    Ok(())
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

fn next_reply_userdata() -> Result<u64, String> {
    NEXT_HOOK_USERDATA
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| "Plugin mpv hook userdata space is exhausted".to_string())
}

fn with_session_executor<T>(
    state: &AppState,
    window_label: &str,
    action: impl FnOnce(&mut crate::mpv::MpvExecutor) -> Result<T, String>,
) -> Result<T, String> {
    let session = state.player_session_for_window(window_label)?;
    let mut executor = session
        .mpv_executor()
        .lock()
        .map_err(|error| error.to_string())?;
    action(&mut executor)
}

#[tauri::command]
pub fn plugin_mpv_add_hook(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    name: String,
    priority: i32,
    callback_id: u64,
) -> Result<bool, String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    validate_hook_name(&name)?;
    let window_label = window.label().to_string();
    let key = RegistrationKey {
        window_label: window_label.clone(),
        identifier,
        callback_id,
    };
    let reply_userdata = next_reply_userdata()?;

    {
        let mut registry = registry().lock().map_err(|error| error.to_string())?;
        if registry.by_callback.contains_key(&key) {
            return Err("Plugin mpv hook callback ID is already registered".to_string());
        }
        let active_count = registry
            .by_userdata
            .values()
            .filter(|registration| {
                registration.active
                    && registration.key.window_label == key.window_label
                    && registration.key.identifier == key.identifier
            })
            .count();
        if active_count >= MAX_ACTIVE_HOOKS_PER_PLUGIN_WINDOW {
            return Err(format!(
                "Plugins may register at most {MAX_ACTIVE_HOOKS_PER_PLUGIN_WINDOW} active mpv hooks per player"
            ));
        }
        let lifetime_count = registry
            .by_userdata
            .keys()
            .filter(|(label, _)| label == &window_label)
            .count();
        if lifetime_count >= MAX_LIFETIME_HOOKS_PER_WINDOW {
            return Err(format!(
                "This player has reached the {MAX_LIFETIME_HOOKS_PER_WINDOW} lifetime mpv hook limit"
            ));
        }
        let registration = HookRegistration {
            key: key.clone(),
            reply_userdata,
            active: true,
        };
        registry
            .by_userdata
            .insert((window_label.clone(), reply_userdata), registration);
        registry.by_callback.insert(key.clone(), reply_userdata);
        registry.known_windows.insert(window_label.clone());
    }

    if let Err(error) = with_session_executor(state.inner(), &window_label, |executor| {
        executor.add_hook(&name, priority, reply_userdata)
    }) {
        let mut registry = registry().lock().map_err(|lock| lock.to_string())?;
        registry
            .by_userdata
            .remove(&(window_label.clone(), reply_userdata));
        registry.by_callback.remove(&key);
        if !registry
            .by_userdata
            .keys()
            .any(|(label, _)| label == &window_label)
        {
            registry.known_windows.remove(&window_label);
        }
        return Err(error);
    }
    Ok(true)
}

#[tauri::command]
pub fn plugin_mpv_continue_hook(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    callback_id: u64,
    hook_id: u64,
) -> Result<bool, String> {
    let window_label = window.label().to_string();
    let should_continue = {
        let mut registry = registry().lock().map_err(|error| error.to_string())?;
        let Some(reply_userdata) = registry
            .pending
            .get(&(window_label.clone(), hook_id))
            .copied()
        else {
            return Ok(false);
        };
        let authorized = registry
            .by_userdata
            .get(&(window_label.clone(), reply_userdata))
            .is_some_and(|registration| {
                registration.active
                    && registration.key.identifier == identifier
                    && registration.key.callback_id == callback_id
            });
        if authorized {
            registry.pending.remove(&(window_label.clone(), hook_id));
        }
        authorized
    };
    if !should_continue {
        return Ok(false);
    }
    with_session_executor(state.inner(), &window_label, |executor| {
        executor.continue_hook(hook_id)
    })?;
    Ok(true)
}

#[tauri::command]
pub fn plugin_mpv_remove_hooks(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
) -> Result<usize, String> {
    deactivate_matching(
        state.inner(),
        |registration| {
            registration.key.window_label == window.label()
                && registration.key.identifier == identifier
        },
        false,
    )
}

fn deactivate_matching(
    state: &AppState,
    predicate: impl Fn(&HookRegistration) -> bool,
    remove: bool,
) -> Result<usize, String> {
    let (matched, pending) = {
        let mut registry = registry().lock().map_err(|error| error.to_string())?;
        let matched = registry
            .by_userdata
            .iter()
            .filter_map(|(storage_key, registration)| {
                predicate(registration).then(|| (storage_key.clone(), registration.key.clone()))
            })
            .collect::<Vec<_>>();
        let matched_userdata = matched
            .iter()
            .map(|((window_label, userdata), _)| (window_label.clone(), *userdata))
            .collect::<HashSet<_>>();
        for (storage_key, key) in &matched {
            registry.by_callback.remove(key);
            if remove {
                registry.by_userdata.remove(storage_key);
            } else if let Some(registration) = registry.by_userdata.get_mut(storage_key) {
                registration.active = false;
            }
        }
        let pending = registry
            .pending
            .iter()
            .filter_map(|((window_label, hook_id), userdata)| {
                matched_userdata
                    .contains(&(window_label.clone(), *userdata))
                    .then(|| (window_label.clone(), *hook_id))
            })
            .collect::<Vec<_>>();
        for (window_label, hook_id) in &pending {
            registry.pending.remove(&(window_label.clone(), *hook_id));
        }
        if remove {
            registry.known_windows = registry
                .by_userdata
                .keys()
                .map(|(window_label, _)| window_label.clone())
                .collect();
        }
        (matched.len(), pending)
    };

    for (window_label, hook_id) in pending {
        // Cleanup is best effort: a closing player may have destroyed its libmpv client already.
        let _ = with_session_executor(state, &window_label, |executor| {
            executor.continue_hook(hook_id)
        });
    }
    Ok(matched)
}

pub fn stop_identifier(state: &AppState, identifier: &str) {
    let _ = deactivate_matching(
        state,
        |registration| registration.key.identifier == identifier,
        false,
    );
}

pub fn stop_window(state: &AppState, window_label: &str) {
    let _ = deactivate_matching(
        state,
        |registration| registration.key.window_label == window_label,
        true,
    );
}

pub fn stop_all(state: &AppState) {
    let _ = deactivate_matching(state, |_| true, true);
}

pub fn dispatch_pending(app: &AppHandle, state: &AppState) {
    let windows = registry()
        .lock()
        .map(|registry| registry.known_windows.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for window_label in windows {
        let events = with_session_executor(state, &window_label, |executor| {
            Ok(executor.take_pending_hook_events())
        })
        .unwrap_or_default();
        for event in events {
            let Some(hook) = event.hook else {
                continue;
            };
            let registration = {
                let mut registry = match registry().lock() {
                    Ok(registry) => registry,
                    Err(_) => {
                        let _ = with_session_executor(state, &window_label, |executor| {
                            executor.continue_hook(hook.id)
                        });
                        continue;
                    }
                };
                let registration = registry
                    .by_userdata
                    .get(&(window_label.clone(), event.reply_userdata))
                    .filter(|registration| registration.active)
                    .cloned();
                if registration.is_some() {
                    registry
                        .pending
                        .insert((window_label.clone(), hook.id), event.reply_userdata);
                }
                registration
            };

            let Some(registration) = registration else {
                let _ = with_session_executor(state, &window_label, |executor| {
                    executor.continue_hook(hook.id)
                });
                continue;
            };
            let payload = HookEventPayload {
                identifier: registration.key.identifier,
                window_label: window_label.clone(),
                callback_id: registration.key.callback_id,
                hook_id: hook.id,
                name: hook.name,
            };
            if app.emit_to(&window_label, HOOK_EVENT, payload).is_err() {
                if let Ok(mut registry) = registry().lock() {
                    registry.pending.remove(&(window_label.clone(), hook.id));
                }
                let _ = with_session_executor(state, &window_label, |executor| {
                    executor.continue_hook(hook.id)
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_names_are_bounded_and_nul_safe() {
        assert!(validate_hook_name("on_load").is_ok());
        assert!(validate_hook_name("").is_err());
        assert!(validate_hook_name("on\0load").is_err());
        assert!(validate_hook_name(&"x".repeat(MAX_HOOK_NAME_BYTES + 1)).is_err());
    }

    #[test]
    fn deactivation_plan_identifies_only_matching_pending_hooks() {
        let key = RegistrationKey {
            window_label: "player-test".to_string(),
            identifier: "io.iina.test".to_string(),
            callback_id: 7,
        };
        let registration = HookRegistration {
            key: key.clone(),
            reply_userdata: 42,
            active: true,
        };
        let mut registry = HookRegistry::default();
        registry
            .by_userdata
            .insert((key.window_label.clone(), 42), registration);
        registry.by_callback.insert(key.clone(), 42);
        registry.pending.insert((key.window_label.clone(), 99), 42);
        registry.pending.insert((key.window_label.clone(), 100), 43);

        let matched = registry
            .pending
            .iter()
            .filter_map(|((window_label, hook_id), userdata)| {
                (*userdata == 42).then(|| (window_label.clone(), *hook_id))
            })
            .collect::<Vec<_>>();
        assert_eq!(matched, vec![("player-test".to_string(), 99)]);
    }
}
