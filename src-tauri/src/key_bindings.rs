use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const USER_INPUT_CONFIG_DIRECTORY: &str = "input_conf";

const CONFIG_EXTENSION: &str = "conf";
const MATERIALIZED_BUILTIN_DIRECTORY: &str = ".builtins";
const MAX_PROFILE_NAME_BYTES: usize = 250;
const IINA_DEFAULT_CONTENTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../参考/iina/iina/config/iina-default-input.conf"
));
const MPV_DEFAULT_CONTENTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../参考/iina/iina/config/input.conf"
));
const VLC_DEFAULT_CONTENTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../参考/iina/iina/config/vlc-default-input.conf"
));
const MOVIST_DEFAULT_CONTENTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../参考/iina/iina/config/movist-default-input.conf"
));

static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveKeyBinding {
    pub normalized_mpv_key: String,
    pub action: Vec<String>,
    pub is_iina_command: bool,
}

/// Resolves the active input configuration into the same last-key-wins view used by mpv and
/// IINA's Key Binding preference pane. `null` is IINA's bundled-default sentinel; an explicit
/// array, including an empty one, is the complete user model.
pub fn active_key_bindings_from_preference(modeled: Option<&Value>) -> Vec<ActiveKeyBinding> {
    let parsed = match modeled {
        None | Some(Value::Null) => parse_input_conf_for_menu(IINA_DEFAULT_CONTENTS),
        Some(Value::Array(rows)) => rows
            .iter()
            .filter_map(active_key_binding_from_value)
            .collect(),
        Some(_) => Vec::new(),
    };
    last_key_wins(parsed)
}

fn active_key_binding_from_value(value: &Value) -> Option<ActiveKeyBinding> {
    let row = value.as_object()?;
    let raw_key = row.get("rawKey")?.as_str()?;
    let raw_action = row
        .get("rawAction")
        .and_then(Value::as_str)
        .or_else(|| row.get("rawCommand").and_then(Value::as_str))?;
    let is_iina_command = row
        .get("isIINACommand")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let raw_action = if is_iina_command {
        raw_action
            .strip_prefix("#@iina ")
            .map(str::trim)
            .unwrap_or(raw_action)
    } else {
        raw_action
    };
    active_key_binding(raw_key, raw_action, is_iina_command)
}

fn parse_input_conf_for_menu(contents: &str) -> Vec<ActiveKeyBinding> {
    contents
        .lines()
        .filter_map(|line| {
            let mut line = line.trim_start();
            let is_iina_command = line.starts_with("#@iina");
            if is_iina_command {
                line = line.trim_start_matches("#@iina").trim_start();
            } else if line.starts_with('#') {
                return None;
            }
            let line = line.split_once('#').map_or(line, |(command, _)| command);
            let mut fields = line.trim().splitn(2, char::is_whitespace);
            let raw_key = fields.next()?;
            let raw_action = fields.next()?.trim();
            active_key_binding(raw_key, raw_action, is_iina_command)
        })
        .collect()
}

fn active_key_binding(
    raw_key: &str,
    raw_action: &str,
    is_iina_command: bool,
) -> Option<ActiveKeyBinding> {
    let mut action = raw_action
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if action.is_empty() {
        return None;
    }
    if action[0].starts_with('{') {
        if action[0] == "{default}" {
            action.remove(0);
        } else {
            return None;
        }
    }
    if action.is_empty() || (raw_key == "default-bindings" && action == ["start"]) {
        return None;
    }
    Some(ActiveKeyBinding {
        normalized_mpv_key: normalize_mpv_key(raw_key),
        action,
        is_iina_command,
    })
}

fn last_key_wins(bindings: Vec<ActiveKeyBinding>) -> Vec<ActiveKeyBinding> {
    let mut active = Vec::<ActiveKeyBinding>::new();
    for binding in bindings {
        if let Some(index) = active
            .iter()
            .position(|candidate| candidate.normalized_mpv_key == binding.normalized_mpv_key)
        {
            active[index] = binding;
        } else {
            active.push(binding);
        }
    }
    active
}

pub(crate) fn normalize_mpv_key(raw_key: &str) -> String {
    if raw_key == "default-bindings" || raw_key.matches('-').count() > 1 {
        return raw_key.to_string();
    }
    if raw_key == "+" {
        return "PLUS".to_string();
    }

    let expanded = raw_key.replace("++", "+PLUS");
    let mut parts = expanded.split('+').collect::<Vec<_>>();
    let mut key = parts.pop().unwrap_or_default().to_string();
    key = match key.as_str() {
        "#" => "SHARP".to_string(),
        "+" => "PLUS".to_string(),
        _ if key.chars().count() > 1 => key.to_uppercase(),
        _ => key,
    };

    let mut control = false;
    let mut option = false;
    let mut shift = false;
    let mut command = false;
    let mut other = Vec::new();
    for modifier in parts {
        if modifier.eq_ignore_ascii_case("shift") {
            if let Some(shifted) = shifted_mpv_key(&key) {
                key = shifted.to_string();
            } else if !is_shifted_mpv_key(&key) {
                shift = true;
            }
        } else if modifier.eq_ignore_ascii_case("meta") {
            command = true;
        } else if modifier.eq_ignore_ascii_case("ctrl") {
            control = true;
        } else if modifier.eq_ignore_ascii_case("alt") {
            option = true;
        } else if !modifier.is_empty() {
            other.push(modifier.to_uppercase());
        }
    }

    let mut normalized = Vec::new();
    if control {
        normalized.push("Ctrl".to_string());
    }
    if option {
        normalized.push("Alt".to_string());
    }
    if shift {
        normalized.push("Shift".to_string());
    }
    if command {
        normalized.push("Meta".to_string());
    }
    normalized.extend(other);
    normalized.push(key);
    normalized.join("+")
}

fn shifted_mpv_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "a" => "A",
        "b" => "B",
        "c" => "C",
        "d" => "D",
        "e" => "E",
        "f" => "F",
        "g" => "G",
        "h" => "H",
        "i" => "I",
        "j" => "J",
        "k" => "K",
        "l" => "L",
        "m" => "M",
        "n" => "N",
        "o" => "O",
        "p" => "P",
        "q" => "Q",
        "r" => "R",
        "s" => "S",
        "t" => "T",
        "u" => "U",
        "v" => "V",
        "w" => "W",
        "x" => "X",
        "y" => "Y",
        "z" => "Z",
        "1" => "!",
        "2" => "@",
        "3" => "SHARP",
        "4" => "$",
        "5" => "%",
        "6" => "^",
        "7" => "&",
        "8" => "*",
        "9" => "(",
        "0" => ")",
        "=" => "PLUS",
        "-" => "_",
        "]" => "}",
        "[" => "{",
        "'" => "\"",
        ";" => ":",
        "\\" => "|",
        "," => "<",
        "/" => "?",
        "." => ">",
        "`" => "~",
        _ => return None,
    })
}

fn is_shifted_mpv_key(key: &str) -> bool {
    matches!(
        key,
        "A" | "B"
            | "C"
            | "D"
            | "E"
            | "F"
            | "G"
            | "H"
            | "I"
            | "J"
            | "K"
            | "L"
            | "M"
            | "N"
            | "O"
            | "P"
            | "Q"
            | "R"
            | "S"
            | "T"
            | "U"
            | "V"
            | "W"
            | "X"
            | "Y"
            | "Z"
            | "!"
            | "@"
            | "SHARP"
            | "$"
            | "%"
            | "^"
            | "&"
            | "*"
            | "("
            | ")"
            | "PLUS"
            | "_"
            | "}"
            | "{"
            | "\""
            | ":"
            | "|"
            | "<"
            | "?"
            | ">"
            | "~"
    )
}

#[derive(Debug, Clone, Copy)]
struct BuiltinProfile {
    name: &'static str,
    file_name: &'static str,
    contents: &'static str,
}

const BUILTIN_PROFILES: [BuiltinProfile; 4] = [
    BuiltinProfile {
        name: "IINA Default",
        file_name: "iina-default-input.conf",
        contents: IINA_DEFAULT_CONTENTS,
    },
    BuiltinProfile {
        name: "mpv Default",
        file_name: "input.conf",
        contents: MPV_DEFAULT_CONTENTS,
    },
    BuiltinProfile {
        name: "VLC Default",
        file_name: "vlc-default-input.conf",
        contents: VLC_DEFAULT_CONTENTS,
    },
    BuiltinProfile {
        name: "Movist Default",
        file_name: "movist-default-input.conf",
        contents: MOVIST_DEFAULT_CONTENTS,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyBindingProfileKind {
    Builtin,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBindingProfile {
    pub name: String,
    pub file_name: String,
    pub kind: KeyBindingProfileKind,
    pub read_only: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBindingProfileDocument {
    pub profile: KeyBindingProfile,
    pub contents: String,
}

#[derive(Debug)]
pub enum KeyBindingRepositoryError {
    InvalidName {
        name: String,
        reason: &'static str,
    },
    Conflict {
        name: String,
    },
    NotFound {
        name: String,
    },
    ReadOnly {
        name: String,
    },
    InvalidImport {
        path: PathBuf,
        reason: &'static str,
    },
    UnsafePath {
        path: PathBuf,
        reason: &'static str,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for KeyBindingRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName { name, reason } => write!(
                formatter,
                "KEY_BINDING_PROFILE_INVALID_NAME: {name:?}: {reason}"
            ),
            Self::Conflict { name } => write!(
                formatter,
                "KEY_BINDING_PROFILE_CONFLICT: a profile named {name:?} already exists"
            ),
            Self::NotFound { name } => write!(
                formatter,
                "KEY_BINDING_PROFILE_NOT_FOUND: no profile named {name:?} exists"
            ),
            Self::ReadOnly { name } => write!(
                formatter,
                "KEY_BINDING_PROFILE_READ_ONLY: built-in profile {name:?} cannot be modified"
            ),
            Self::InvalidImport { path, reason } => write!(
                formatter,
                "KEY_BINDING_PROFILE_INVALID_IMPORT: {}: {reason}",
                path.display()
            ),
            Self::UnsafePath { path, reason } => write!(
                formatter,
                "KEY_BINDING_PROFILE_UNSAFE_PATH: {}: {reason}",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "KEY_BINDING_PROFILE_IO: failed to {operation} {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for KeyBindingRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type KeyBindingRepositoryResult<T> = Result<T, KeyBindingRepositoryError>;

#[derive(Debug, Clone)]
pub struct KeyBindingRepository {
    user_directory: PathBuf,
}

impl KeyBindingRepository {
    pub fn new(app_config_directory: impl AsRef<Path>) -> Self {
        Self {
            user_directory: app_config_directory
                .as_ref()
                .join(USER_INPUT_CONFIG_DIRECTORY),
        }
    }

    #[cfg(test)]
    fn user_directory(&self) -> &Path {
        &self.user_directory
    }

    pub fn list_profiles(&self) -> KeyBindingRepositoryResult<Vec<KeyBindingProfile>> {
        let mut profiles = BUILTIN_PROFILES
            .iter()
            .map(|profile| builtin_descriptor(*profile))
            .collect::<Vec<_>>();
        profiles.extend(self.list_user_profiles()?);
        Ok(profiles)
    }

    pub fn read_profile(
        &self,
        name: &str,
    ) -> KeyBindingRepositoryResult<KeyBindingProfileDocument> {
        validate_profile_name(name)?;
        if let Some(profile) = builtin_profile(name) {
            return Ok(KeyBindingProfileDocument {
                profile: builtin_descriptor(profile),
                contents: profile.contents.to_string(),
            });
        }

        let profile = self.find_user_profile(name)?;
        let path = profile_path(&profile)?;
        ensure_regular_user_file(path)?;
        let contents = fs::read_to_string(path)
            .map_err(|error| io_error("read", path.to_path_buf(), error))?;
        Ok(KeyBindingProfileDocument { profile, contents })
    }

    pub fn create_empty_profile(
        &self,
        name: &str,
    ) -> KeyBindingRepositoryResult<KeyBindingProfileDocument> {
        self.create_profile_with_contents(name, "")
    }

    pub fn duplicate_profile(
        &self,
        source_name: &str,
        new_name: &str,
    ) -> KeyBindingRepositoryResult<KeyBindingProfileDocument> {
        let source = self.read_profile(source_name)?;
        self.create_profile_with_contents(new_name, &source.contents)
    }

    pub fn import_profile(
        &self,
        source_path: impl AsRef<Path>,
        requested_name: Option<&str>,
    ) -> KeyBindingRepositoryResult<KeyBindingProfileDocument> {
        let source_path = source_path.as_ref();
        if source_path.extension() != Some(OsStr::new(CONFIG_EXTENSION)) {
            return Err(KeyBindingRepositoryError::InvalidImport {
                path: source_path.to_path_buf(),
                reason: "input configuration files must use the .conf extension",
            });
        }
        let metadata = fs::metadata(source_path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                KeyBindingRepositoryError::InvalidImport {
                    path: source_path.to_path_buf(),
                    reason: "source file does not exist",
                }
            } else {
                io_error("inspect import source", source_path.to_path_buf(), error)
            }
        })?;
        if !metadata.is_file() {
            return Err(KeyBindingRepositoryError::InvalidImport {
                path: source_path.to_path_buf(),
                reason: "source path is not a regular file",
            });
        }
        let derived_name = source_path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or_else(|| KeyBindingRepositoryError::InvalidImport {
                path: source_path.to_path_buf(),
                reason: "source filename is not valid UTF-8",
            })?;
        let name = requested_name.unwrap_or(derived_name);
        validate_profile_name(name)?;
        let contents = fs::read_to_string(source_path).map_err(|error| {
            if error.kind() == io::ErrorKind::InvalidData {
                KeyBindingRepositoryError::InvalidImport {
                    path: source_path.to_path_buf(),
                    reason: "source file is not valid UTF-8",
                }
            } else {
                io_error("read import source", source_path.to_path_buf(), error)
            }
        })?;
        self.create_profile_with_contents(name, &contents)
    }

    pub fn save_profile(
        &self,
        name: &str,
        contents: &str,
    ) -> KeyBindingRepositoryResult<KeyBindingProfileDocument> {
        validate_profile_name(name)?;
        if let Some(profile) = builtin_profile(name) {
            return Err(KeyBindingRepositoryError::ReadOnly {
                name: profile.name.to_string(),
            });
        }
        let profile = self.find_user_profile(name)?;
        let path = profile_path(&profile)?;
        ensure_regular_user_file(path)?;
        write_atomic(path, contents.as_bytes())?;
        Ok(KeyBindingProfileDocument {
            profile,
            contents: contents.to_string(),
        })
    }

    pub fn delete_profile(&self, name: &str) -> KeyBindingRepositoryResult<()> {
        validate_profile_name(name)?;
        if let Some(profile) = builtin_profile(name) {
            return Err(KeyBindingRepositoryError::ReadOnly {
                name: profile.name.to_string(),
            });
        }
        let profile = self.find_user_profile(name)?;
        let path = profile_path(&profile)?;
        ensure_regular_user_file(path)?;
        fs::remove_file(path).map_err(|error| io_error("delete", path.to_path_buf(), error))
    }

    pub fn reveal_path(&self, name: &str) -> KeyBindingRepositoryResult<PathBuf> {
        validate_profile_name(name)?;
        if let Some(profile) = builtin_profile(name) {
            return Err(KeyBindingRepositoryError::ReadOnly {
                name: profile.name.to_string(),
            });
        }
        let profile = self.find_user_profile(name)?;
        let path = profile_path(&profile)?.to_path_buf();
        ensure_regular_user_file(&path)?;
        Ok(path)
    }

    /// Returns a real input.conf path suitable for mpv's pre-initialize `input-conf` option.
    ///
    /// User profiles already live on disk. Built-ins are compiled into the executable so release
    /// bundles do not depend on the source reference tree; they are materialized under the app
    /// config directory on demand.
    pub fn runtime_path(&self, name: &str) -> KeyBindingRepositoryResult<PathBuf> {
        validate_profile_name(name)?;
        let Some(profile) = builtin_profile(name) else {
            return self.reveal_path(name);
        };

        self.ensure_user_directory()?;
        let directory = self.user_directory.join(MATERIALIZED_BUILTIN_DIRECTORY);
        ensure_real_directory(
            &directory,
            "built-in profile directory must be a real directory and cannot be a symlink",
        )?;
        let path = directory.join(profile.file_name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(KeyBindingRepositoryError::UnsafePath {
                    path,
                    reason: "materialized built-in profile must be a regular file",
                });
            }
            Ok(_) => {
                let current =
                    fs::read(&path).map_err(|error| io_error("read", path.clone(), error))?;
                if current != profile.contents.as_bytes() {
                    write_atomic(&path, profile.contents.as_bytes())?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                write_atomic(&path, profile.contents.as_bytes())?;
            }
            Err(error) => return Err(io_error("inspect", path, error)),
        }
        Ok(path)
    }

    fn create_profile_with_contents(
        &self,
        name: &str,
        contents: &str,
    ) -> KeyBindingRepositoryResult<KeyBindingProfileDocument> {
        validate_profile_name(name)?;
        if builtin_profile(name).is_some()
            || self
                .list_user_profiles()?
                .iter()
                .any(|profile| names_match(&profile.name, name))
        {
            return Err(KeyBindingRepositoryError::Conflict {
                name: name.to_string(),
            });
        }
        self.ensure_user_directory()?;
        let path = self.user_path_for_name(name);
        write_new(&path, contents.as_bytes(), name)?;
        let profile = user_descriptor(name, &path);
        Ok(KeyBindingProfileDocument {
            profile,
            contents: contents.to_string(),
        })
    }

    fn find_user_profile(&self, name: &str) -> KeyBindingRepositoryResult<KeyBindingProfile> {
        self.list_user_profiles()?
            .into_iter()
            .find(|profile| names_match(&profile.name, name))
            .ok_or_else(|| KeyBindingRepositoryError::NotFound {
                name: name.to_string(),
            })
    }

    fn list_user_profiles(&self) -> KeyBindingRepositoryResult<Vec<KeyBindingProfile>> {
        self.ensure_user_directory()?;
        let entries = fs::read_dir(&self.user_directory)
            .map_err(|error| io_error("list", self.user_directory.to_path_buf(), error))?;
        let mut profiles = Vec::new();
        let mut names_by_key = BTreeMap::<String, String>::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                io_error(
                    "read directory entry from",
                    self.user_directory.to_path_buf(),
                    error,
                )
            })?;
            let path = entry.path();
            if path.extension() != Some(OsStr::new(CONFIG_EXTENSION)) {
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|error| io_error("inspect", path.clone(), error))?;
            if file_type.is_symlink() || !file_type.is_file() {
                return Err(KeyBindingRepositoryError::UnsafePath {
                    path,
                    reason: "profile entries must be regular files and cannot be symlinks",
                });
            }
            let name = path.file_stem().and_then(OsStr::to_str).ok_or_else(|| {
                KeyBindingRepositoryError::UnsafePath {
                    path: path.clone(),
                    reason: "profile filename is not valid UTF-8",
                }
            })?;
            validate_profile_name(name)?;
            if builtin_profile(name).is_some() {
                return Err(KeyBindingRepositoryError::Conflict {
                    name: name.to_string(),
                });
            }
            let key = canonical_name(name);
            if let Some(existing_name) = names_by_key.insert(key, name.to_string()) {
                return Err(KeyBindingRepositoryError::Conflict {
                    name: format!("{existing_name} / {name}"),
                });
            }
            profiles.push(user_descriptor(name, &path));
        }
        profiles.sort_by(|left, right| {
            canonical_name(&left.name)
                .cmp(&canonical_name(&right.name))
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(profiles)
    }

    fn user_path_for_name(&self, name: &str) -> PathBuf {
        self.user_directory.join(format!("{name}.conf"))
    }

    fn ensure_user_directory(&self) -> KeyBindingRepositoryResult<()> {
        ensure_real_directory(
            &self.user_directory,
            "input_conf must be a real directory and cannot be a symlink",
        )
    }
}

fn ensure_real_directory(
    path: &Path,
    unsafe_reason: &'static str,
) -> KeyBindingRepositoryResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(KeyBindingRepositoryError::UnsafePath {
                    path: path.to_path_buf(),
                    reason: unsafe_reason,
                });
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)
                .map_err(|error| io_error("create profile directory", path.to_path_buf(), error))?;
            let metadata = fs::symlink_metadata(path).map_err(|error| {
                io_error("inspect profile directory", path.to_path_buf(), error)
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(KeyBindingRepositoryError::UnsafePath {
                    path: path.to_path_buf(),
                    reason: unsafe_reason,
                });
            }
            Ok(())
        }
        Err(error) => Err(io_error(
            "inspect profile directory",
            path.to_path_buf(),
            error,
        )),
    }
}

fn validate_profile_name(name: &str) -> KeyBindingRepositoryResult<()> {
    let invalid = |reason| KeyBindingRepositoryError::InvalidName {
        name: name.to_string(),
        reason,
    };
    if name.is_empty() {
        return Err(invalid("profile name cannot be empty"));
    }
    if name != name.trim() {
        return Err(invalid("profile name cannot start or end with whitespace"));
    }
    if name == "." || name == ".." {
        return Err(invalid("profile name cannot be a path component"));
    }
    if name.as_bytes().len() > MAX_PROFILE_NAME_BYTES {
        return Err(invalid("profile name is too long"));
    }
    if name.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
            )
    }) {
        return Err(invalid("profile name contains invalid filename characters"));
    }
    if name.ends_with('.') {
        return Err(invalid("profile name cannot end with a period"));
    }
    let device_stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let reserved_device_name = matches!(device_stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (device_stem.len() == 4
            && (device_stem.starts_with("COM") || device_stem.starts_with("LPT"))
            && device_stem.as_bytes()[3].is_ascii_digit()
            && device_stem.as_bytes()[3] != b'0');
    if reserved_device_name {
        return Err(invalid("profile name is reserved by the filesystem"));
    }
    Ok(())
}

fn builtin_profile(name: &str) -> Option<BuiltinProfile> {
    BUILTIN_PROFILES
        .iter()
        .copied()
        .find(|profile| names_match(profile.name, name))
}

fn builtin_descriptor(profile: BuiltinProfile) -> KeyBindingProfile {
    KeyBindingProfile {
        name: profile.name.to_string(),
        file_name: profile.file_name.to_string(),
        kind: KeyBindingProfileKind::Builtin,
        read_only: true,
        path: None,
    }
}

fn user_descriptor(name: &str, path: &Path) -> KeyBindingProfile {
    KeyBindingProfile {
        name: name.to_string(),
        file_name: path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_string(),
        kind: KeyBindingProfileKind::User,
        read_only: false,
        path: Some(path.to_string_lossy().into_owned()),
    }
}

fn profile_path(profile: &KeyBindingProfile) -> KeyBindingRepositoryResult<&Path> {
    profile
        .path
        .as_deref()
        .map(Path::new)
        .ok_or_else(|| KeyBindingRepositoryError::ReadOnly {
            name: profile.name.clone(),
        })
}

fn names_match(left: &str, right: &str) -> bool {
    canonical_name(left) == canonical_name(right)
}

fn canonical_name(name: &str) -> String {
    name.to_lowercase()
}

fn ensure_regular_user_file(path: &Path) -> KeyBindingRepositoryResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(KeyBindingRepositoryError::UnsafePath {
                path: path.to_path_buf(),
                reason: "profile path must be a regular file and cannot be a symlink",
            })
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(KeyBindingRepositoryError::NotFound {
                name: path
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        }
        Err(error) => Err(io_error("inspect", path.to_path_buf(), error)),
    }
}

fn write_new(path: &Path, contents: &[u8], name: &str) -> KeyBindingRepositoryResult<()> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(KeyBindingRepositoryError::Conflict {
                name: name.to_string(),
            });
        }
        Err(error) => return Err(io_error("create", path.to_path_buf(), error)),
    };
    if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(io_error("write", path.to_path_buf(), error));
    }
    Ok(())
}

fn write_atomic(path: &Path, contents: &[u8]) -> KeyBindingRepositoryResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| KeyBindingRepositoryError::UnsafePath {
            path: path.to_path_buf(),
            reason: "profile file has no parent directory",
        })?;
    let file_name = path.file_name().and_then(OsStr::to_str).ok_or_else(|| {
        KeyBindingRepositoryError::UnsafePath {
            path: path.to_path_buf(),
            reason: "profile filename is not valid UTF-8",
        }
    })?;

    let (temporary_path, mut temporary_file) = loop {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_path = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => break (temporary_path, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(io_error("create temporary file for", temporary_path, error));
            }
        }
    };

    if let Err(error) = temporary_file
        .write_all(contents)
        .and_then(|_| temporary_file.sync_all())
    {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary_path);
        return Err(io_error(
            "write temporary file for",
            path.to_path_buf(),
            error,
        ));
    }
    drop(temporary_file);
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(io_error("replace", path.to_path_buf(), error));
    }
    Ok(())
}

fn io_error(
    operation: &'static str,
    path: PathBuf,
    source: io::Error,
) -> KeyBindingRepositoryError {
    KeyBindingRepositoryError::Io {
        operation,
        path,
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "iima-key-bindings-{}-{sequence}-{label}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("test config directory should be created");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn exposes_the_four_reference_builtins_in_reference_order() {
        let directory = TestDirectory::new("builtins");
        let repository = KeyBindingRepository::new(directory.path());

        let profiles = repository.list_profiles().expect("profiles should list");

        assert_eq!(profiles.len(), 4);
        assert_eq!(
            profiles
                .iter()
                .map(|profile| profile.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "IINA Default",
                "mpv Default",
                "VLC Default",
                "Movist Default"
            ]
        );
        assert!(profiles.iter().all(|profile| profile.read_only));
        assert!(profiles.iter().all(|profile| profile.path.is_none()));
        assert_eq!(
            repository
                .read_profile("IINA Default")
                .expect("IINA profile should read")
                .contents,
            IINA_DEFAULT_CONTENTS
        );
        assert_eq!(
            repository
                .read_profile("mpv Default")
                .expect("mpv profile should read")
                .contents,
            MPV_DEFAULT_CONTENTS
        );
        assert_eq!(
            repository
                .read_profile("VLC Default")
                .expect("VLC profile should read")
                .contents,
            VLC_DEFAULT_CONTENTS
        );
        assert_eq!(
            repository
                .read_profile("Movist Default")
                .expect("Movist profile should read")
                .contents,
            MOVIST_DEFAULT_CONTENTS
        );
    }

    #[test]
    fn resolves_the_bundled_iina_profile_for_native_menu_equivalents() {
        let bindings = active_key_bindings_from_preference(None);

        assert!(bindings.iter().any(|binding| {
            binding.normalized_mpv_key == "Meta+s"
                && binding.action == ["screenshot"]
                && !binding.is_iina_command
        }));
        assert!(bindings.iter().any(|binding| {
            binding.normalized_mpv_key == "Meta+V"
                && binding.action == ["video-panel"]
                && binding.is_iina_command
        }));
        assert!(bindings.iter().any(|binding| {
            binding.normalized_mpv_key == "Ctrl+Meta+p"
                && binding.action == ["toggle-pip"]
                && binding.is_iina_command
        }));
    }

    #[test]
    fn explicit_empty_key_binding_model_does_not_fall_back_to_defaults() {
        assert!(active_key_bindings_from_preference(Some(&serde_json::json!([]))).is_empty());
    }

    #[test]
    fn modeled_bindings_are_last_key_wins_and_skip_inactive_sections() {
        let modeled = serde_json::json!([
            {"rawKey": "Meta+x", "rawAction": "seek 5", "isIINACommand": false},
            {"rawKey": "Meta+y", "rawAction": "{video} seek 9", "isIINACommand": false},
            {"rawKey": "Alt+界", "rawAction": "{default} seek 7 relative+exact", "isIINACommand": false},
            {"rawKey": "Meta+x", "rawAction": "seek 12 relative", "isIINACommand": false}
        ]);

        let bindings = active_key_bindings_from_preference(Some(&modeled));

        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].normalized_mpv_key, "Meta+x");
        assert_eq!(bindings[0].action, ["seek", "12", "relative"]);
        assert_eq!(bindings[1].normalized_mpv_key, "Alt+界");
        assert_eq!(bindings[1].action, ["seek", "7", "relative+exact"]);
    }

    #[test]
    fn accepts_legacy_prefixed_iina_raw_commands() {
        let modeled = serde_json::json!([{
            "rawKey": "Meta+界",
            "rawCommand": "#@iina video-panel",
            "isIINACommand": true
        }]);

        assert_eq!(
            active_key_bindings_from_preference(Some(&modeled)),
            vec![ActiveKeyBinding {
                normalized_mpv_key: "Meta+界".to_string(),
                action: vec!["video-panel".to_string()],
                is_iina_command: true,
            }]
        );
    }

    #[test]
    fn materializes_builtins_and_uses_empty_user_profiles_as_real_runtime_inputs() {
        let directory = TestDirectory::new("runtime-paths");
        let repository = KeyBindingRepository::new(directory.path());

        let builtin_path = repository
            .runtime_path("IINA Default")
            .expect("built-in profile should materialize");
        assert_eq!(
            fs::read_to_string(&builtin_path).expect("materialized profile should read"),
            IINA_DEFAULT_CONTENTS
        );
        assert!(builtin_path.ends_with("input_conf/.builtins/iina-default-input.conf"));

        let empty = repository
            .create_empty_profile("No Bindings")
            .expect("empty profile should create");
        let empty_path = repository
            .runtime_path(&empty.profile.name)
            .expect("empty user profile should resolve");
        assert_eq!(
            fs::read_to_string(empty_path).expect("empty profile should read"),
            ""
        );
    }

    #[test]
    fn supports_create_save_duplicate_list_read_and_delete() {
        let directory = TestDirectory::new("crud");
        let repository = KeyBindingRepository::new(directory.path());

        let created = repository
            .create_empty_profile("Personal")
            .expect("empty profile should be created");
        assert_eq!(created.contents, "");
        assert_eq!(created.profile.kind, KeyBindingProfileKind::User);
        assert!(!created.profile.read_only);

        let saved = repository
            .save_profile("Personal", "SPACE cycle pause\n# comment\n")
            .expect("profile should save");
        assert_eq!(saved.contents, "SPACE cycle pause\n# comment\n");
        assert_eq!(
            repository
                .read_profile("personal")
                .expect("profile lookup should be case insensitive")
                .contents,
            saved.contents
        );

        let duplicate = repository
            .duplicate_profile("Personal", "Personal Copy")
            .expect("profile should duplicate");
        assert_eq!(duplicate.contents, saved.contents);
        assert_eq!(duplicate.profile.file_name, "Personal Copy.conf");

        let names = repository
            .list_profiles()
            .expect("profiles should list")
            .into_iter()
            .map(|profile| profile.name)
            .collect::<Vec<_>>();
        assert_eq!(
            &names[4..],
            &["Personal".to_string(), "Personal Copy".to_string()]
        );

        let reveal_path = repository
            .reveal_path("Personal")
            .expect("user profile path should resolve");
        assert_eq!(
            reveal_path,
            repository.user_directory().join("Personal.conf")
        );

        repository
            .delete_profile("Personal")
            .expect("profile should delete");
        assert!(matches!(
            repository.read_profile("Personal"),
            Err(KeyBindingRepositoryError::NotFound { .. })
        ));
    }

    #[test]
    fn duplicates_a_builtin_into_an_editable_user_profile() {
        let directory = TestDirectory::new("duplicate-builtin");
        let repository = KeyBindingRepository::new(directory.path());

        let duplicate = repository
            .duplicate_profile("IINA Default", "My IINA Keys")
            .expect("built-in should duplicate");

        assert_eq!(duplicate.contents, IINA_DEFAULT_CONTENTS);
        assert_eq!(duplicate.profile.kind, KeyBindingProfileKind::User);
        assert!(!duplicate.profile.read_only);
    }

    #[test]
    fn imports_conf_files_and_rejects_duplicate_names() {
        let directory = TestDirectory::new("import");
        let source = directory.path().join("Imported.conf");
        fs::write(&source, "x seek 5\n").expect("import fixture should write");
        let repository = KeyBindingRepository::new(directory.path().join("config"));

        let imported = repository
            .import_profile(&source, None)
            .expect("profile should import");
        assert_eq!(imported.profile.name, "Imported");
        assert_eq!(imported.contents, "x seek 5\n");
        assert!(matches!(
            repository.import_profile(&source, None),
            Err(KeyBindingRepositoryError::Conflict { .. })
        ));
        assert!(matches!(
            repository.create_empty_profile("imported"),
            Err(KeyBindingRepositoryError::Conflict { .. })
        ));
        assert!(matches!(
            repository.create_empty_profile("iina default"),
            Err(KeyBindingRepositoryError::Conflict { .. })
        ));
    }

    #[test]
    fn rejects_read_write_delete_and_reveal_mutations_for_builtins() {
        let directory = TestDirectory::new("read-only");
        let repository = KeyBindingRepository::new(directory.path());

        assert!(matches!(
            repository.save_profile("IINA Default", "changed"),
            Err(KeyBindingRepositoryError::ReadOnly { .. })
        ));
        assert!(matches!(
            repository.delete_profile("mpv Default"),
            Err(KeyBindingRepositoryError::ReadOnly { .. })
        ));
        assert!(matches!(
            repository.reveal_path("VLC Default"),
            Err(KeyBindingRepositoryError::ReadOnly { .. })
        ));
    }

    #[test]
    fn blocks_path_traversal_and_invalid_filenames() {
        let directory = TestDirectory::new("traversal");
        let repository = KeyBindingRepository::new(directory.path().join("config"));
        let outside = directory.path().join("escaped.conf");

        for name in [
            "../escaped",
            "nested/profile",
            "nested\\profile",
            ".",
            "..",
            " trailing",
            "NUL",
        ] {
            assert!(matches!(
                repository.create_empty_profile(name),
                Err(KeyBindingRepositoryError::InvalidName { .. })
            ));
        }
        assert!(!outside.exists());

        let wrong_extension = directory.path().join("input.txt");
        fs::write(&wrong_extension, "x seek 5\n").expect("fixture should write");
        assert!(matches!(
            repository.import_profile(&wrong_extension, None),
            Err(KeyBindingRepositoryError::InvalidImport { .. })
        ));
    }

    #[test]
    fn empty_profiles_are_valid_for_create_save_import_and_roundtrip() {
        let directory = TestDirectory::new("empty");
        let repository = KeyBindingRepository::new(directory.path().join("config"));
        let source = directory.path().join("Blank.conf");
        fs::write(&source, "").expect("blank import fixture should write");

        repository
            .create_empty_profile("Empty")
            .expect("empty profile should create");
        assert_eq!(
            repository
                .save_profile("Empty", "")
                .expect("empty content should save")
                .contents,
            ""
        );
        assert_eq!(
            repository
                .read_profile("Empty")
                .expect("empty profile should read")
                .contents,
            ""
        );
        assert_eq!(
            repository
                .import_profile(&source, None)
                .expect("empty config should import")
                .contents,
            ""
        );
    }

    #[test]
    fn preserves_utf8_and_line_endings_through_atomic_save_roundtrip() {
        let directory = TestDirectory::new("roundtrip");
        let repository = KeyBindingRepository::new(directory.path());
        let contents = "# 自定义配置\r\nSPACE cycle pause\r\nMeta+ß show-text \"✓\"\n";

        repository
            .create_empty_profile("自定义")
            .expect("unicode profile should create");
        repository
            .save_profile("自定义", contents)
            .expect("unicode content should save");

        assert_eq!(
            repository
                .read_profile("自定义")
                .expect("unicode profile should read")
                .contents,
            contents
        );
        let temporary_files = fs::read_dir(repository.user_directory())
            .expect("profile directory should list")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(temporary_files, 0);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_profile_files_and_repository_directories() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new("symlinks");
        let target = directory.path().join("target.conf");
        fs::write(&target, "x seek 5\n").expect("target fixture should write");

        let config = directory.path().join("config");
        let repository = KeyBindingRepository::new(&config);
        fs::create_dir_all(repository.user_directory()).expect("input_conf should create");
        symlink(&target, repository.user_directory().join("Linked.conf"))
            .expect("profile symlink should create");
        assert!(matches!(
            repository.list_profiles(),
            Err(KeyBindingRepositoryError::UnsafePath { .. })
        ));

        let linked_config = directory.path().join("linked-config");
        fs::create_dir_all(&linked_config).expect("linked config root should create");
        symlink(
            directory.path(),
            linked_config.join(USER_INPUT_CONFIG_DIRECTORY),
        )
        .expect("directory symlink should create");
        let linked_repository = KeyBindingRepository::new(&linked_config);
        assert!(matches!(
            linked_repository.list_profiles(),
            Err(KeyBindingRepositoryError::UnsafePath { .. })
        ));
    }

    #[test]
    fn error_messages_expose_stable_error_categories() {
        let directory = TestDirectory::new("errors");
        let repository = KeyBindingRepository::new(directory.path());

        let error = repository
            .create_empty_profile("../bad")
            .expect_err("invalid name should fail");
        assert!(error
            .to_string()
            .starts_with("KEY_BINDING_PROFILE_INVALID_NAME:"));
        let error = repository
            .delete_profile("Missing")
            .expect_err("missing profile should fail");
        assert!(error
            .to_string()
            .starts_with("KEY_BINDING_PROFILE_NOT_FOUND:"));
    }
}
