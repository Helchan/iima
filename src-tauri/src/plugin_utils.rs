use crate::commands::plugin_file_path_for_command;
use crate::localization;
use crate::native_prompt;
use crate::plugins;
use crate::state::AppState;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

const MAX_PLUGIN_EXEC_ARGUMENTS: usize = 256;
const MAX_PLUGIN_EXEC_ARGUMENT_BYTES: usize = 64 * 1024;
const MAX_PLUGIN_EXEC_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginExecResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginExecOutputPayload {
    identifier: String,
    role: String,
    request_id: String,
    stream: &'static str,
    chunk: String,
}

type PluginExecOutputHook = Arc<dyn Fn(&'static str, &[u8]) + Send + Sync>;

fn ensure_enabled(app: &AppHandle, identifier: &str) -> Result<(), String> {
    if plugins::plugin_is_enabled(app, identifier)? {
        Ok(())
    } else {
        Err("Plugin is not enabled".to_string())
    }
}

fn require_file_system(app: &AppHandle, identifier: &str) -> Result<(), String> {
    plugins::require_plugin_permission(app, identifier, "file-system")
}

fn validate_identifier_and_text(identifier: &str, label: &str, value: &str) -> Result<(), String> {
    if identifier.is_empty() || identifier.len() > 256 || identifier.contains('\0') {
        return Err("Plugin identifier is invalid".to_string());
    }
    if value.len() > 4096 || value.contains('\0') {
        return Err(format!("Plugin {label} is invalid"));
    }
    Ok(())
}

fn validate_instance_role(role: &str) -> Result<(), String> {
    match role {
        "entry" | "global" => Ok(()),
        _ => Err("Plugin utils role must be entry or global".to_string()),
    }
}

fn configured_binary_directory(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("bin"))
        .map_err(|error| error.to_string())
}

fn bundled_executable_directory() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Application executable directory is unavailable".to_string())
}

fn bundled_binary(app: &AppHandle, file: &str) -> Result<Option<PathBuf>, String> {
    for directory in [
        configured_binary_directory(app)?,
        bundled_executable_directory()?,
    ] {
        let candidate = directory.join(file);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn validate_exec_arguments(args: &[String]) -> Result<(), String> {
    if args.len() > MAX_PLUGIN_EXEC_ARGUMENTS
        || args.iter().map(String::len).sum::<usize>() > MAX_PLUGIN_EXEC_ARGUMENT_BYTES
        || args.iter().any(|value| value.contains('\0'))
    {
        return Err("Plugin process arguments exceed the supported limits".to_string());
    }
    Ok(())
}

fn collect_capped_output_with_hook(
    mut reader: impl Read,
    maximum: usize,
    stream: &'static str,
    output_hook: Option<PluginExecOutputHook>,
) -> Result<Vec<u8>, String> {
    let mut kept = Vec::with_capacity(maximum.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    let mut exceeded = false;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        if let Some(output_hook) = output_hook.as_ref() {
            output_hook(stream, &buffer[..count]);
        }
        let remaining = maximum.saturating_sub(kept.len());
        let retained = count.min(remaining);
        kept.extend_from_slice(&buffer[..retained]);
        exceeded |= retained < count;
    }
    if exceeded {
        Err("Plugin process output exceeds the 8 MiB limit".to_string())
    } else {
        Ok(kept)
    }
}

#[cfg(test)]
fn collect_capped_output(reader: impl Read, maximum: usize) -> Result<Vec<u8>, String> {
    collect_capped_output_with_hook(reader, maximum, "stdout", None)
}

fn make_private_binary_executable(path: &Path, is_private: bool) -> Result<(), String> {
    if !is_private || !path.is_file() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        if metadata.permissions().mode() & 0o111 == 0 {
            let mut permissions = metadata.permissions();
            permissions.set_mode(permissions.mode() | 0o755);
            fs::set_permissions(path, permissions).map_err(|error| {
                format!(
                    "The binary is not executable, and execute permission cannot be added: {error}"
                )
            })?;
        }
    }
    Ok(())
}

fn run_plugin_process(
    executable: PathBuf,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    uses_system_lookup: bool,
    output_hook: Option<PluginExecOutputHook>,
) -> Result<PluginExecResult, String> {
    let mut process = if uses_system_lookup {
        let mut process = Command::new("/bin/bash");
        process.arg("-c").arg("exec \"$0\" \"$@\"").arg(executable);
        process
    } else {
        Command::new(executable)
    };
    process
        .args(args)
        .env_clear()
        .env("LC_ALL", "en_US.UTF-8")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    let mut child = process
        .spawn()
        .map_err(|error| format!("Cannot launch plugin process: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Plugin process stdout is unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Plugin process stderr is unavailable".to_string())?;
    let stdout_hook = output_hook.clone();
    let stderr_hook = output_hook;
    let stdout_reader = std::thread::spawn(move || {
        collect_capped_output_with_hook(stdout, MAX_PLUGIN_EXEC_OUTPUT_BYTES, "stdout", stdout_hook)
    });
    let stderr_reader = std::thread::spawn(move || {
        collect_capped_output_with_hook(stderr, MAX_PLUGIN_EXEC_OUTPUT_BYTES, "stderr", stderr_hook)
    });
    let status = child
        .wait()
        .map_err(|error| format!("Cannot wait for plugin process: {error}"))?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Plugin process stdout reader panicked".to_string())??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Plugin process stderr reader panicked".to_string())??;
    Ok(PluginExecResult {
        status: status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

#[tauri::command]
pub fn plugin_utils_resolve_path(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
) -> Result<String, String> {
    validate_identifier_and_text(&identifier, "path", &path)?;
    require_file_system(&app, &identifier)?;
    let session = state.inner().player_session_for_window(window.label())?;
    let resolved = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    Ok(resolved.path.display().to_string())
}

#[tauri::command]
pub fn plugin_utils_file_in_path(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    file: String,
) -> Result<bool, String> {
    validate_identifier_and_text(&identifier, "binary path", &file)?;
    require_file_system(&app, &identifier)?;
    if file.is_empty() {
        return Ok(false);
    }
    if !file.contains('/') && !file.starts_with('@') && !file.starts_with('~') {
        if bundled_binary(&app, &file)?.is_some() {
            return Ok(true);
        }
    }
    let session = state.inner().player_session_for_window(window.label())?;
    plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &file)
        .map(|path| path.path.exists())
}

#[tauri::command]
pub async fn plugin_utils_exec(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    window: WebviewWindow,
    identifier: String,
    role: String,
    file: String,
    args: Vec<String>,
    cwd: Option<String>,
    request_id: Option<String>,
) -> Result<PluginExecResult, String> {
    validate_identifier_and_text(&identifier, "binary path", &file)?;
    validate_instance_role(&role)?;
    validate_exec_arguments(&args)?;
    require_file_system(&app, &identifier)?;
    let session = state.inner().player_session_for_window(window.label())?;
    let local_binary = if !file.contains('/') && !file.starts_with('@') && !file.starts_with('~') {
        bundled_binary(&app, &file)?
    } else {
        None
    };
    let (executable, is_private, uses_system_lookup) = if let Some(path) = local_binary {
        (path, false, false)
    } else if !file.contains('/') && !file.starts_with('@') && !file.starts_with('~') {
        (PathBuf::from(&file), false, true)
    } else {
        let resolved =
            plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &file)?;
        if !resolved.path.is_file() {
            return Err(format!("Cannot find the binary {file}"));
        }
        (resolved.path, resolved.is_private, false)
    };
    make_private_binary_executable(&executable, is_private)?;
    let cwd = cwd
        .filter(|value| !value.is_empty())
        .map(|value| {
            validate_identifier_and_text(&identifier, "working directory", &value)?;
            plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &value)
                .map(|path| path.path)
        })
        .transpose()?;
    if cwd.as_ref().is_some_and(|path| !path.is_dir()) {
        return Err("Plugin process working directory is not a directory".to_string());
    }
    let output_hook = request_id
        .filter(|request_id| {
            !request_id.is_empty()
                && request_id.len() <= 128
                && request_id.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
        })
        .map(|request_id| {
            let app = app.clone();
            let identifier = identifier.clone();
            let role = role.clone();
            let window_label = window.label().to_string();
            Arc::new(move |stream: &'static str, bytes: &[u8]| {
                let _ = app.emit_to(
                    &window_label,
                    "iima-plugin-utils-exec-output",
                    PluginExecOutputPayload {
                        identifier: identifier.clone(),
                        role: role.clone(),
                        request_id: request_id.clone(),
                        stream,
                        chunk: String::from_utf8_lossy(bytes).into_owned(),
                    },
                );
            }) as PluginExecOutputHook
        });

    tauri::async_runtime::spawn_blocking(move || {
        run_plugin_process(executable, args, cwd, uses_system_lookup, output_hook)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn plugin_utils_ask(app: AppHandle, identifier: String, title: String) -> Result<bool, String> {
    validate_identifier_and_text(&identifier, "alert title", &title)?;
    ensure_enabled(&app, &identifier)?;
    native_prompt::confirm(
        &title,
        &localization::menu_title("OK"),
        &localization::menu_title("Cancel"),
    )
}

#[tauri::command]
pub fn plugin_utils_prompt(
    app: AppHandle,
    identifier: String,
    title: String,
) -> Result<Option<String>, String> {
    validate_identifier_and_text(&identifier, "prompt title", &title)?;
    ensure_enabled(&app, &identifier)?;
    native_prompt::prompt_multiline_text(
        &title,
        "",
        "",
        &localization::menu_title("OK"),
        &localization::menu_title("Cancel"),
    )
}

#[tauri::command]
pub fn plugin_utils_choose_file(
    app: AppHandle,
    identifier: String,
    title: String,
    choose_dir: Option<bool>,
    allowed_file_types: Option<Vec<String>>,
) -> Result<Option<String>, String> {
    validate_identifier_and_text(&identifier, "file chooser title", &title)?;
    ensure_enabled(&app, &identifier)?;
    let allowed_file_types = allowed_file_types.unwrap_or_default();
    if allowed_file_types.len() > 64
        || allowed_file_types.iter().any(|extension| {
            extension.is_empty()
                || extension.len() > 32
                || !extension
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
    {
        return Err("Plugin file chooser contains invalid allowed file types".to_string());
    }
    let mut dialog = app
        .dialog()
        .file()
        .set_title(title)
        .set_can_create_directories(false);
    if !allowed_file_types.is_empty() {
        let extensions = allowed_file_types
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        dialog = dialog.add_filter("Files", &extensions);
    }
    let selected = if choose_dir.unwrap_or(false) {
        dialog.blocking_pick_folder()
    } else {
        dialog.blocking_pick_file()
    };
    selected
        .map(|path| {
            path.into_path()
                .map(|path| path.display().to_string())
                .map_err(|error| error.to_string())
        })
        .transpose()
}

#[tauri::command]
pub fn plugin_utils_open(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    url: String,
) -> Result<bool, String> {
    validate_identifier_and_text(&identifier, "open URL", &url)?;
    ensure_enabled(&app, &identifier)?;
    if let Ok(parsed) = tauri::Url::parse(&url) {
        if matches!(parsed.scheme(), "http" | "https") {
            return Command::new("/usr/bin/open")
                .arg(&url)
                .spawn()
                .map(|_| true)
                .map_err(|error| format!("Cannot open URL: {error}"));
        }
    }
    let session = state.inner().player_session_for_window(window.label())?;
    let path = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &url)?;
    Command::new("/usr/bin/open")
        .arg(path.path)
        .spawn()
        .map(|_| true)
        .map_err(|error| format!("Cannot open plugin path: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{
        collect_capped_output, run_plugin_process, validate_exec_arguments, validate_instance_role,
        MAX_PLUGIN_EXEC_ARGUMENTS,
    };
    use std::io::Cursor;

    #[test]
    fn process_output_limit_never_returns_a_truncated_success() {
        assert_eq!(
            collect_capped_output(Cursor::new(b"IINA"), 4).unwrap(),
            b"IINA"
        );
        assert!(collect_capped_output(Cursor::new(b"IINA!"), 4).is_err());
    }

    #[test]
    fn process_argument_limits_are_bounded_and_reject_nul() {
        assert!(validate_exec_arguments(&["--version".to_string()]).is_ok());
        assert!(validate_exec_arguments(&["bad\0argument".to_string()]).is_err());
        assert!(
            validate_exec_arguments(&vec!["x".to_string(); MAX_PLUGIN_EXEC_ARGUMENTS + 1]).is_err()
        );
    }

    #[test]
    fn exec_output_owners_are_entry_or_global_instances() {
        assert!(validate_instance_role("entry").is_ok());
        assert!(validate_instance_role("global").is_ok());
        assert!(validate_instance_role("child").is_err());
    }

    #[test]
    fn process_runner_preserves_arguments_and_captures_both_streams() {
        let chunks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let received = chunks.clone();
        let hook: super::PluginExecOutputHook = std::sync::Arc::new(move |stream, bytes| {
            received.lock().unwrap().push((
                stream.to_string(),
                String::from_utf8_lossy(bytes).into_owned(),
            ));
        });
        let result = run_plugin_process(
            std::path::PathBuf::from("/bin/sh"),
            vec![
                "-c".to_string(),
                "printf '%s' \"$1\"; printf '%s' \"$2\" >&2; exit 7".to_string(),
                "plugin-test".to_string(),
                "hello world".to_string(),
                "warning".to_string(),
            ],
            None,
            false,
            Some(hook),
        )
        .unwrap();
        assert_eq!(result.status, 7);
        assert_eq!(result.stdout, "hello world");
        assert_eq!(result.stderr, "warning");
        let chunks = chunks.lock().unwrap();
        assert!(chunks
            .iter()
            .any(|(stream, chunk)| { stream == "stdout" && chunk.contains("hello world") }));
        assert!(chunks
            .iter()
            .any(|(stream, chunk)| stream == "stderr" && chunk.contains("warning")));
        drop(chunks);

        let system = run_plugin_process(
            std::path::PathBuf::from("printf"),
            vec!["%s".to_string(), "system lookup".to_string()],
            None,
            true,
            None,
        )
        .unwrap();
        assert_eq!(system.status, 0);
        assert_eq!(system.stdout, "system lookup");
    }
}
