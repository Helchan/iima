use crate::commands;
use crate::plugins;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

pub const CONTROLLER_MESSAGE_EVENT: &str = "iima-plugin-global-controller-message";
pub const CHILD_MESSAGE_EVENT: &str = "iima-plugin-global-child-message";
const MAX_MANAGED_INSTANCES_PER_PLUGIN: usize = 32;
const MAX_MESSAGE_NAME_BYTES: usize = 256;
const MAX_MESSAGE_DATA_BYTES: usize = 1024 * 1024;
const MAX_USER_LABEL_BYTES: usize = 256;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginGlobalCreateOptions {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub disable_window_animation: bool,
    #[serde(default)]
    pub disable_ui: bool,
    #[serde(default)]
    pub enable_plugins: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControllerMessagePayload {
    identifier: String,
    name: String,
    data: Value,
    sender: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChildMessagePayload {
    identifier: String,
    name: String,
    data: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedInstance {
    window_label: String,
    reference_label: String,
    user_label: Option<String>,
}

#[derive(Default)]
struct GlobalRegistry {
    controllers: HashMap<String, String>,
    instances: HashMap<String, BTreeMap<u64, ManagedInstance>>,
    next_instance_ids: HashMap<String, u64>,
}

fn registry() -> &'static Mutex<GlobalRegistry> {
    static REGISTRY: OnceLock<Mutex<GlobalRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(GlobalRegistry::default()))
}

fn validate_global_plugin(app: &AppHandle, identifier: &str) -> Result<(), String> {
    let spec = plugins::runtime_specs(app)?
        .into_iter()
        .find(|spec| spec.identifier == identifier)
        .ok_or_else(|| format!("Plugin {identifier} is not enabled"))?;
    if spec.global_entry.is_none() {
        return Err(format!("Plugin {identifier} does not declare globalEntry"));
    }
    Ok(())
}

fn validate_controller(
    registry: &GlobalRegistry,
    identifier: &str,
    window_label: &str,
) -> Result<(), String> {
    match registry.controllers.get(identifier) {
        Some(label) if label == window_label => Ok(()),
        Some(_) => Err("Plugin global controller belongs to another window".to_string()),
        None => Err("Plugin global controller is not registered".to_string()),
    }
}

fn validate_child_window(window_label: &str) -> Result<(), String> {
    if window_label == "main" || window_label.starts_with("player-") {
        Ok(())
    } else {
        Err("Plugin global child APIs belong to player windows".to_string())
    }
}

fn reference_sender_label(
    registry: &GlobalRegistry,
    identifier: &str,
    window_label: &str,
) -> String {
    registry
        .instances
        .get(identifier)
        .and_then(|instances| {
            instances
                .values()
                .find(|instance| instance.window_label == window_label)
        })
        .map(|instance| instance.reference_label.clone())
        .unwrap_or_else(|| window_label.to_string())
}

fn validate_message(name: &str, data: &Value) -> Result<(), String> {
    if name.as_bytes().len() > MAX_MESSAGE_NAME_BYTES || name.contains('\0') {
        return Err(format!(
            "Plugin global message names must be at most {MAX_MESSAGE_NAME_BYTES} bytes and cannot contain NUL"
        ));
    }
    let size = serde_json::to_vec(data)
        .map_err(|error| format!("Plugin global message is not valid JSON: {error}"))?
        .len();
    if size > MAX_MESSAGE_DATA_BYTES {
        return Err(format!(
            "Plugin global message exceeds the {MAX_MESSAGE_DATA_BYTES} byte limit"
        ));
    }
    Ok(())
}

fn validate_user_label(label: Option<String>) -> Result<Option<String>, String> {
    match label {
        Some(label) if label.as_bytes().len() > MAX_USER_LABEL_BYTES || label.contains('\0') => {
            Err(format!(
                "Plugin player labels must be at most {MAX_USER_LABEL_BYTES} bytes and cannot contain NUL"
            ))
        }
        label => Ok(label),
    }
}

fn resolve_open_target(
    app: &AppHandle,
    identifier: &str,
    raw: Option<String>,
) -> Result<Vec<String>, String> {
    let Some(raw) = raw.filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    if raw.starts_with("@tmp/") || raw.starts_with("@data/") {
        return plugins::resolve_plugin_file_path(app, identifier, &raw, None)
            .map(|path| vec![path.path.display().to_string()]);
    }
    plugins::require_plugin_permission(app, identifier, "file-system")?;
    if tauri::Url::parse(&raw).is_ok() {
        return Ok(vec![raw]);
    }
    if raw.starts_with("~/") || std::path::Path::new(&raw).is_absolute() {
        return plugins::resolve_plugin_file_path(app, identifier, &raw, None)
            .map(|path| vec![path.path.display().to_string()]);
    }
    Ok(vec![raw])
}

#[tauri::command]
pub fn plugin_global_register_controller(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Plugin global controllers belong to the main window".to_string());
    }
    validate_global_plugin(&app, &identifier)?;
    let mut registry = registry().lock().map_err(|error| error.to_string())?;
    if let Some(existing) = registry.controllers.get(&identifier) {
        if existing == window.label() {
            return Ok(());
        }
        return Err("Plugin global controller is already registered".to_string());
    }
    registry
        .controllers
        .insert(identifier, window.label().to_string());
    Ok(())
}

#[tauri::command]
pub fn plugin_global_unregister_controller(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
) -> Result<(), String> {
    let labels = {
        let mut registry = registry().lock().map_err(|error| error.to_string())?;
        validate_controller(&registry, &identifier, window.label())?;
        registry.controllers.remove(&identifier);
        registry.next_instance_ids.remove(&identifier);
        registry
            .instances
            .remove(&identifier)
            .unwrap_or_default()
            .into_values()
            .map(|instance| instance.window_label)
            .collect::<Vec<_>>()
    };
    close_windows(&app, labels);
    Ok(())
}

#[tauri::command]
pub fn plugin_global_create_player_instance(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    instance_id: u64,
    options: PluginGlobalCreateOptions,
) -> Result<u64, String> {
    validate_global_plugin(&app, &identifier)?;
    let user_label = validate_user_label(options.label.clone())?;
    let paths = resolve_open_target(&app, &identifier, options.url.clone())?;
    let player_label = {
        let mut registry = registry().lock().map_err(|error| error.to_string())?;
        validate_controller(&registry, &identifier, window.label())?;
        if registry
            .instances
            .get(&identifier)
            .map(BTreeMap::len)
            .unwrap_or(0)
            >= MAX_MANAGED_INSTANCES_PER_PLUGIN
        {
            return Err(format!(
                "Plugin {identifier} reached the {MAX_MANAGED_INSTANCES_PER_PLUGIN} managed-player limit"
            ));
        }
        let next = registry
            .next_instance_ids
            .entry(identifier.clone())
            .or_default();
        let expected = next
            .checked_add(1)
            .ok_or_else(|| "Plugin managed-player identifier overflow".to_string())?;
        if instance_id != expected {
            return Err(format!(
                "Plugin managed-player identifier must be the next value ({expected})"
            ));
        }
        *next = instance_id;
        let (player_label, _) = state.inner().create_player_session()?;
        registry
            .instances
            .entry(identifier.clone())
            .or_default()
            .insert(
                instance_id,
                ManagedInstance {
                    window_label: player_label.clone(),
                    reference_label: format!("{instance_id}-{identifier}"),
                    user_label: user_label.clone(),
                },
            );
        player_label
    };

    let result = commands::open_plugin_managed_player_window(
        &app,
        state.inner(),
        player_label.clone(),
        &identifier,
        instance_id,
        user_label.as_deref(),
        options.enable_plugins,
        options.disable_ui,
        options.disable_window_animation,
        paths,
    );
    if let Err(error) = result {
        if let Ok(mut registry) = registry().lock() {
            if let Some(instances) = registry.instances.get_mut(&identifier) {
                instances.remove(&instance_id);
            }
        }
        return Err(error);
    }
    Ok(instance_id)
}

#[tauri::command]
pub fn plugin_global_get_label(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
) -> Result<Option<String>, String> {
    validate_child_window(window.label())?;
    if !plugins::plugin_is_enabled(&app, &identifier)? {
        return Err("Plugin is not enabled".to_string());
    }
    let registry = registry().lock().map_err(|error| error.to_string())?;
    Ok(registry
        .instances
        .get(&identifier)
        .and_then(|instances| {
            instances
                .values()
                .find(|instance| instance.window_label == window.label())
        })
        .and_then(|instance| instance.user_label.clone()))
}

#[tauri::command]
pub fn plugin_global_post_to_controller(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    name: String,
    data: Value,
) -> Result<(), String> {
    validate_child_window(window.label())?;
    validate_global_plugin(&app, &identifier)?;
    validate_message(&name, &data)?;
    let controller = registry().lock().map_err(|error| error.to_string())?;
    let controller_label = controller
        .controllers
        .get(&identifier)
        .cloned()
        .ok_or_else(|| "Plugin global controller is not registered".to_string())?;
    let sender = reference_sender_label(&controller, &identifier, window.label());
    drop(controller);
    app.emit_to(
        controller_label,
        CONTROLLER_MESSAGE_EVENT,
        ControllerMessagePayload {
            identifier,
            name,
            data,
            sender,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn plugin_global_post_to_child(
    app: AppHandle,
    window: WebviewWindow,
    identifier: String,
    target: Option<Value>,
    name: String,
    data: Value,
) -> Result<usize, String> {
    validate_global_plugin(&app, &identifier)?;
    validate_message(&name, &data)?;
    let labels = {
        let registry = registry().lock().map_err(|error| error.to_string())?;
        validate_controller(&registry, &identifier, window.label())?;
        resolve_child_targets(&registry, &identifier, target.as_ref())?
    };
    let payload = ChildMessagePayload {
        identifier,
        name,
        data,
    };
    let mut delivered = 0;
    for label in labels {
        if app.get_webview_window(&label).is_none() {
            continue;
        }
        app.emit_to(&label, CHILD_MESSAGE_EVENT, &payload)
            .map_err(|error| error.to_string())?;
        delivered += 1;
    }
    Ok(delivered)
}

fn resolve_child_targets(
    registry: &GlobalRegistry,
    identifier: &str,
    target: Option<&Value>,
) -> Result<Vec<String>, String> {
    let instances = registry.instances.get(identifier);
    match target {
        None | Some(Value::Null) => Ok(instances
            .into_iter()
            .flat_map(|instances| instances.values())
            .map(|instance| instance.window_label.clone())
            .collect()),
        Some(Value::Number(value)) => {
            let id = value
                .as_u64()
                .ok_or_else(|| "Plugin global target must be a positive integer".to_string())?;
            Ok(instances
                .and_then(|instances| instances.get(&id))
                .map(|instance| vec![instance.window_label.clone()])
                .unwrap_or_default())
        }
        Some(Value::String(label)) => Ok(instances
            .and_then(|instances| {
                instances
                    .values()
                    .find(|instance| instance.reference_label == *label)
            })
            .map(|instance| vec![instance.window_label.clone()])
            .unwrap_or_else(|| {
                if label == "main" || label.starts_with("player-") {
                    vec![label.clone()]
                } else {
                    Vec::new()
                }
            })),
        _ => Err(
            "Plugin global target must be null, a managed-player ID, or a player label".to_string(),
        ),
    }
}

pub fn remove_window(window_label: &str) {
    let Ok(mut registry) = registry().lock() else {
        return;
    };
    for instances in registry.instances.values_mut() {
        instances.retain(|_, instance| instance.window_label != window_label);
    }
    registry
        .instances
        .retain(|_, instances| !instances.is_empty());
}

pub fn stop_identifier<R: Runtime>(app: &AppHandle<R>, identifier: &str) {
    let labels = {
        let Ok(mut registry) = registry().lock() else {
            return;
        };
        registry.controllers.remove(identifier);
        registry.next_instance_ids.remove(identifier);
        registry
            .instances
            .remove(identifier)
            .unwrap_or_default()
            .into_values()
            .map(|instance| instance.window_label)
            .collect::<Vec<_>>()
    };
    close_windows(app, labels);
}

pub fn stop_all<R: Runtime>(app: &AppHandle<R>) {
    let labels = {
        let Ok(mut registry) = registry().lock() else {
            return;
        };
        registry.controllers.clear();
        registry.next_instance_ids.clear();
        registry
            .instances
            .drain()
            .flat_map(|(_, instances)| instances.into_values())
            .map(|instance| instance.window_label)
            .collect::<Vec<_>>()
    };
    close_windows(app, labels);
}

fn close_windows<R: Runtime>(app: &AppHandle<R>, labels: Vec<String>) {
    for label in labels {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        reference_sender_label, resolve_child_targets, validate_message, validate_user_label,
        GlobalRegistry, ManagedInstance, MAX_MESSAGE_DATA_BYTES,
    };
    use serde_json::{json, Value};

    fn registry_fixture() -> GlobalRegistry {
        let mut registry = GlobalRegistry::default();
        registry
            .controllers
            .insert("io.iina.test".into(), "main".into());
        registry.instances.insert(
            "io.iina.test".into(),
            [
                (
                    1,
                    ManagedInstance {
                        window_label: "player-1".into(),
                        reference_label: "1-io.iina.test".into(),
                        user_label: Some("first".into()),
                    },
                ),
                (
                    2,
                    ManagedInstance {
                        window_label: "player-2".into(),
                        reference_label: "2-io.iina.test".into(),
                        user_label: None,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        );
        registry
    }

    #[test]
    fn global_targets_match_iina_controller_semantics() {
        let registry = registry_fixture();
        assert_eq!(
            resolve_child_targets(&registry, "io.iina.test", None).unwrap(),
            vec!["player-1", "player-2"]
        );
        assert_eq!(
            resolve_child_targets(&registry, "io.iina.test", Some(&json!(2))).unwrap(),
            vec!["player-2"]
        );
        assert!(
            resolve_child_targets(&registry, "io.iina.test", Some(&json!(99)))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            resolve_child_targets(&registry, "io.iina.test", Some(&json!("2-io.iina.test")))
                .unwrap(),
            vec!["player-2"]
        );
        assert_eq!(
            resolve_child_targets(&registry, "io.iina.test", Some(&json!("main"))).unwrap(),
            vec!["main"]
        );
        assert!(
            resolve_child_targets(&registry, "io.iina.test", Some(&json!("preferences")))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            reference_sender_label(&registry, "io.iina.test", "player-1"),
            "1-io.iina.test"
        );
        assert_eq!(
            reference_sender_label(&registry, "io.iina.test", "main"),
            "main"
        );
    }

    #[test]
    fn global_messages_and_user_labels_are_bounded() {
        validate_message("event", &json!({"ok": true})).unwrap();
        assert!(validate_message(&"x".repeat(257), &Value::Null).is_err());
        assert!(validate_message("event", &json!("x".repeat(MAX_MESSAGE_DATA_BYTES))).is_err());
        assert_eq!(
            validate_user_label(Some("child".into())).unwrap(),
            Some("child".into())
        );
        assert!(validate_user_label(Some("x".repeat(257))).is_err());
    }
}
