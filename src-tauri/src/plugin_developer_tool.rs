use crate::{plugins, state::AppState};
use serde::Serialize;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Mutex, OnceLock};
use tauri::{
    AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

const WINDOW_LABEL_PREFIX: &str = "plugin-developer-tool-";
const CONTEXT_EVENT: &str = "iima-plugin-developer-tool-context";
const OPENED_EVENT: &str = "iima-plugin-developer-tool-opened";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginDeveloperToolContext {
    window_label: String,
    owner_label: String,
    identifier: String,
    role: String,
    context_id: String,
    title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PluginRealmKey {
    owner_label: String,
    identifier: String,
    role: String,
}

impl PluginRealmKey {
    fn new(owner_label: &str, identifier: &str, role: &str) -> Self {
        Self {
            owner_label: owner_label.to_string(),
            identifier: identifier.to_string(),
            role: role.to_string(),
        }
    }
}

#[derive(Default)]
struct PluginDeveloperToolRegistry {
    current_contexts: HashMap<PluginRealmKey, String>,
    window_contexts: HashMap<String, PluginDeveloperToolContext>,
}

impl PluginDeveloperToolRegistry {
    fn set_current_context(&mut self, key: PluginRealmKey, context_id: String) {
        self.current_contexts.insert(key, context_id);
    }

    fn current_context_id(&self, key: &PluginRealmKey) -> Option<&str> {
        self.current_contexts.get(key).map(String::as_str)
    }

    fn retain_window_context(&mut self, context: PluginDeveloperToolContext) -> Result<(), String> {
        if let Some(existing) = self.window_contexts.get(&context.window_label) {
            if existing != &context {
                return Err("Plugin Developer Tool window label collision".to_string());
            }
            return Ok(());
        }
        self.window_contexts
            .insert(context.window_label.clone(), context);
        Ok(())
    }

    fn window_context(&self, label: &str) -> Option<&PluginDeveloperToolContext> {
        self.window_contexts.get(label)
    }
}

fn registry() -> &'static Mutex<PluginDeveloperToolRegistry> {
    static REGISTRY: OnceLock<Mutex<PluginDeveloperToolRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(PluginDeveloperToolRegistry::default()))
}

fn validate_role(role: &str) -> Result<(), String> {
    if matches!(role, "entry" | "global") {
        Ok(())
    } else {
        Err("Plugin Developer Tool role must be entry or global".to_string())
    }
}

fn validate_context_id(context_id: &str) -> Result<(), String> {
    if context_id.is_empty() || context_id.len() > 128 {
        return Err("Plugin Developer Tool contextId must contain 1 to 128 characters".to_string());
    }
    if !context_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(
            "Plugin Developer Tool contextId may only contain ASCII letters, digits, '-', '_', '.', and ':'"
                .to_string(),
        );
    }
    Ok(())
}

fn window_label(owner_label: &str, identifier: &str, role: &str, context_id: &str) -> String {
    // Tauri labels must stay compact and contain only URL-safe characters. FNV-1a is sufficient
    // here because the complete realm/context tuple is retained and collision-checked in REGISTRY.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in format!("{owner_label}\0{identifier}\0{role}\0{context_id}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{WINDOW_LABEL_PREFIX}{hash:016x}")
}

#[tauri::command]
pub fn set_plugin_developer_tool_realm_context(
    window: WebviewWindow,
    identifier: String,
    role: String,
    context_id: String,
) -> Result<(), String> {
    validate_role(&role)?;
    validate_context_id(&context_id)?;

    let app = window.app_handle();
    let state = app.state::<AppState>();
    let owner_label = if role == "global" {
        if window.label() != "main" {
            return Err(
                "The global plugin Developer Tool realm must be registered by main".to_string(),
            );
        }
        "main".to_string()
    } else {
        state
            .player_session_for_window(window.label())?
            .label()
            .to_string()
    };

    let spec = plugins::runtime_specs(app)?
        .into_iter()
        .find(|spec| spec.identifier == identifier)
        .ok_or_else(|| "Plugin runtime is unavailable".to_string())?;
    if role == "global" && spec.global_entry.is_none() {
        return Err("Plugin does not have a global instance".to_string());
    }

    registry()
        .lock()
        .map_err(|error| error.to_string())?
        .set_current_context(
            PluginRealmKey::new(&owner_label, &identifier, &role),
            context_id,
        );
    Ok(())
}

pub fn show_for_owner<R: Runtime>(
    app: &AppHandle<R>,
    owner_label: &str,
    identifier: &str,
    role: &str,
) -> Result<(), String> {
    validate_role(role)?;
    let state = app.state::<AppState>();
    let player_owner = state
        .player_session_for_window(owner_label)?
        .label()
        .to_string();
    let spec = plugins::runtime_specs(app)?
        .into_iter()
        .find(|spec| spec.identifier == identifier)
        .ok_or_else(|| "Plugin runtime is unavailable".to_string())?;
    if role == "global" && spec.global_entry.is_none() {
        return Err("Plugin does not have a global instance".to_string());
    }

    // Global plugin scripts live in the primary player's dedicated global realm. Entry scripts
    // remain bound to the active PlayerCore-equivalent that owned the menu action.
    let runtime_owner = if role == "global" {
        "main".to_string()
    } else {
        player_owner
    };
    state.player_session_for_window(&runtime_owner)?;
    let realm_key = PluginRealmKey::new(&runtime_owner, identifier, role);
    let context_id = registry()
        .lock()
        .map_err(|error| error.to_string())?
        .current_context_id(&realm_key)
        .map(str::to_string)
        .ok_or_else(|| "Plugin Developer Tool realm context is unavailable".to_string())?;
    let display_title = if role == "global" {
        format!("{} Global", spec.name)
    } else {
        spec.name
    };
    let label = window_label(&runtime_owner, identifier, role, &context_id);
    let context = PluginDeveloperToolContext {
        window_label: label.clone(),
        owner_label: runtime_owner.clone(),
        identifier: identifier.to_string(),
        role: role.to_string(),
        context_id,
        title: display_title.clone(),
    };
    registry()
        .lock()
        .map_err(|error| error.to_string())?
        .retain_window_context(context.clone())?;

    if let Some(window) = app.get_webview_window(&label) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        let _ = window.emit(CONTEXT_EVENT, context.clone());
        let _ = app.emit_to(&runtime_owner, OPENED_EVENT, context);
        return Ok(());
    }

    let window =
        WebviewWindowBuilder::new(app, &label, WebviewUrl::App("plugin-devtool.html".into()))
            .title(format!("DevTool: {display_title}"))
            .inner_size(500.0, 400.0)
            .resizable(true)
            .maximizable(true)
            .minimizable(true)
            .decorations(true)
            .center()
            .build()
            .map_err(|error| error.to_string())?;
    let _ = window.emit(CONTEXT_EVENT, context.clone());
    let _ = app.emit_to(&runtime_owner, OPENED_EVENT, context);
    Ok(())
}

#[tauri::command]
pub fn get_plugin_developer_tool_context(
    window: WebviewWindow,
) -> Result<PluginDeveloperToolContext, String> {
    registry()
        .lock()
        .map_err(|error| error.to_string())?
        .window_context(window.label())
        .cloned()
        .ok_or_else(|| "Window is not a managed plugin Developer Tool".to_string())
}

pub fn is_plugin_developer_tool_label(label: &str) -> bool {
    label.starts_with(WINDOW_LABEL_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::{
        is_plugin_developer_tool_label, validate_context_id, validate_role, window_label,
        PluginDeveloperToolContext, PluginDeveloperToolRegistry, PluginRealmKey,
    };

    #[test]
    fn developer_tool_labels_are_stable_and_scoped_to_the_exact_js_context() {
        let entry = window_label("player-1", "io.iina.fixture", "entry", "context-1");
        assert_eq!(
            entry,
            window_label("player-1", "io.iina.fixture", "entry", "context-1")
        );
        assert_ne!(
            entry,
            window_label("player-2", "io.iina.fixture", "entry", "context-1")
        );
        assert_ne!(
            entry,
            window_label("player-1", "io.iina.fixture", "global", "context-1")
        );
        assert_ne!(
            entry,
            window_label("player-1", "io.iina.fixture", "entry", "context-2")
        );
        assert!(is_plugin_developer_tool_label(&entry));
        assert!(!is_plugin_developer_tool_label("inspector"));
    }

    #[test]
    fn realm_registration_advances_current_context_without_discarding_old_windows() {
        let key = PluginRealmKey::new("player-1", "io.iina.fixture", "entry");
        let mut registry = PluginDeveloperToolRegistry::default();

        registry.set_current_context(key.clone(), "context-1".to_string());
        let first_label = window_label(
            &key.owner_label,
            &key.identifier,
            &key.role,
            registry.current_context_id(&key).unwrap(),
        );
        registry
            .retain_window_context(PluginDeveloperToolContext {
                window_label: first_label.clone(),
                owner_label: key.owner_label.clone(),
                identifier: key.identifier.clone(),
                role: key.role.clone(),
                context_id: "context-1".to_string(),
                title: "Fixture".to_string(),
            })
            .unwrap();

        registry.set_current_context(key.clone(), "context-2".to_string());
        let second_label = window_label(
            &key.owner_label,
            &key.identifier,
            &key.role,
            registry.current_context_id(&key).unwrap(),
        );
        registry
            .retain_window_context(PluginDeveloperToolContext {
                window_label: second_label.clone(),
                owner_label: key.owner_label.clone(),
                identifier: key.identifier.clone(),
                role: key.role.clone(),
                context_id: "context-2".to_string(),
                title: "Fixture".to_string(),
            })
            .unwrap();

        assert_ne!(first_label, second_label);
        assert_eq!(registry.current_context_id(&key), Some("context-2"));
        assert_eq!(
            registry.window_context(&first_label).unwrap().context_id,
            "context-1"
        );
        assert_eq!(
            registry.window_context(&second_label).unwrap().context_id,
            "context-2"
        );
    }

    #[test]
    fn role_and_context_id_validation_rejects_ambiguous_identity() {
        assert!(validate_role("entry").is_ok());
        assert!(validate_role("global").is_ok());
        assert!(validate_role("Entry").is_err());
        assert!(validate_role("worker").is_err());

        for valid in [
            "550e8400-e29b-41d4-a716-446655440000",
            "entry:42",
            "realm_1.2",
        ] {
            assert!(validate_context_id(valid).is_ok(), "{valid}");
        }
        for invalid in ["", " context", "context/1", "context 1", "🌍"] {
            assert!(validate_context_id(invalid).is_err(), "{invalid}");
        }
        assert!(validate_context_id(&"x".repeat(129)).is_err());
    }

    #[test]
    fn retained_window_context_cannot_be_overwritten_by_a_hash_collision() {
        let mut registry = PluginDeveloperToolRegistry::default();
        let context = PluginDeveloperToolContext {
            window_label: "plugin-developer-tool-collision".to_string(),
            owner_label: "main".to_string(),
            identifier: "io.iina.fixture".to_string(),
            role: "global".to_string(),
            context_id: "context-1".to_string(),
            title: "Fixture Global".to_string(),
        };
        registry.retain_window_context(context.clone()).unwrap();
        assert!(registry.retain_window_context(context).is_ok());

        let conflicting = PluginDeveloperToolContext {
            window_label: "plugin-developer-tool-collision".to_string(),
            owner_label: "main".to_string(),
            identifier: "io.iina.fixture".to_string(),
            role: "global".to_string(),
            context_id: "context-2".to_string(),
            title: "Fixture Global".to_string(),
        };
        assert!(registry.retain_window_context(conflicting).is_err());
    }

    #[test]
    fn developer_tool_event_context_serializes_the_js_context_identity() {
        let context = PluginDeveloperToolContext {
            window_label: "plugin-developer-tool-fixture".to_string(),
            owner_label: "player-1".to_string(),
            identifier: "io.iina.fixture".to_string(),
            role: "entry".to_string(),
            context_id: "context-42".to_string(),
            title: "Fixture".to_string(),
        };
        let payload = serde_json::to_value(context).unwrap();
        assert_eq!(payload["contextId"], "context-42");
        assert_eq!(payload["ownerLabel"], "player-1");
        assert!(payload.get("context_id").is_none());
    }

    #[test]
    fn realm_context_registration_command_is_exposed_to_webviews() {
        let lib_source = include_str!("lib.rs");
        assert!(
            lib_source
                .matches("set_plugin_developer_tool_realm_context")
                .count()
                >= 2,
            "the realm context command must be imported and registered in generate_handler"
        );
    }
}
