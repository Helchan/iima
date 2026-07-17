use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use md5::{Digest, Md5};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::Url;

use crate::preferences::PreferenceStore;

const OPENSUBTITLES_PROVIDER: &str = ":opensubtitles";
const ASSRT_PROVIDER: &str = ":assrt";
const SHOOTER_PROVIDER: &str = ":shooter";
const OPENSUBTITLES_API_KEY_OBFUSCATED: &str = "SPX87dlUuuHpxeh5u3rd7dHekOT6oYpx";
const OPENSUBTITLES_DEFAULT_API_HOST: &str = "api.opensubtitles.com";
const OPENSUBTITLES_SESSION_LIFETIME: Duration = Duration::from_secs(23 * 60 * 60);
const OPENSUBTITLES_INVALID_TOKEN_ERROR: &str = "OPEN_SUBTITLES_INVALID_TOKEN:";
const ONLINE_SUBTITLE_CANNOT_CONNECT_ERROR: &str = "ONLINE_SUBTITLE_CANNOT_CONNECT:";
const ONLINE_SUBTITLE_NETWORK_ERROR: &str = "ONLINE_SUBTITLE_NETWORK_ERROR:";
const ONLINE_SUBTITLE_TIMED_OUT_ERROR: &str = "ONLINE_SUBTITLE_TIMED_OUT:";
const ONLINE_SUBTITLE_FILE_ERROR: &str = "ONLINE_SUBTITLE_FILE_ERROR:";
const ONLINE_SUBTITLE_CANCELED_ERROR: &str = "ONLINE_SUBTITLE_CANCELED:";
const ASSRT_FALLBACK_TOKEN: &str = "5IzWrb2J099vmA96ECQXwdRSe9xdoBUv";
const CURL_PATH: &str = "/usr/bin/curl";

#[derive(Debug, Clone, Serialize)]
pub struct OnlineSubtitleSearchResult {
    pub provider_id: String,
    pub provider_name: String,
    pub query: String,
    pub candidates: Vec<OnlineSubtitleCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OnlineSubtitleCandidate {
    pub id: String,
    pub name: String,
    pub left: String,
    pub right: String,
}

#[derive(Clone)]
pub struct OpenSubtitlesSession {
    username: String,
    api_base_url: String,
    token: String,
    expires_at: SystemTime,
}

impl OpenSubtitlesSession {
    fn is_valid(&self) -> bool {
        !self.token.is_empty() && SystemTime::now() < self.expires_at
    }

    fn request_context(&self) -> OpenSubtitlesRequestContext {
        OpenSubtitlesRequestContext {
            api_base_url: self.api_base_url.clone(),
            token: Some(self.token.clone()),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self {
            username: "tester".to_string(),
            api_base_url: opensubtitles_api_base(None).unwrap(),
            token: "test-token".to_string(),
            expires_at: SystemTime::now() + Duration::from_secs(60),
        }
    }
}

impl fmt::Debug for OpenSubtitlesSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenSubtitlesSession")
            .field("username", &self.username)
            .field("api_base_url", &self.api_base_url)
            .field("token", &"<private>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct OpenSubtitlesRateLimiter {
    remaining: i64,
    resets_at: SystemTime,
}

impl Default for OpenSubtitlesRateLimiter {
    fn default() -> Self {
        Self {
            remaining: i64::MAX,
            resets_at: UNIX_EPOCH,
        }
    }
}

impl OpenSubtitlesRateLimiter {
    fn delay_before_call_at(&mut self, now: SystemTime) -> Duration {
        if self.resets_at <= now {
            return Duration::ZERO;
        }
        let Ok(until_reset) = self.resets_at.duration_since(now) else {
            return Duration::ZERO;
        };
        self.remaining = self.remaining.saturating_sub(1);
        if self.remaining >= 0 {
            return Duration::ZERO;
        }
        Duration::from_secs(until_reset.as_secs() + u64::from(until_reset.subsec_nanos() != 0))
    }

    fn process_headers_at(&mut self, headers: &BTreeMap<String, String>, now: SystemTime) {
        let (Some(remaining), Some(reset)) = (
            headers.get("ratelimit-remaining"),
            headers.get("ratelimit-reset"),
        ) else {
            return;
        };
        let (Ok(remaining), Ok(reset)) = (remaining.parse::<i64>(), reset.parse::<u64>()) else {
            return;
        };
        if remaining < 0 {
            return;
        }
        let Some(resets_at) = now.checked_add(Duration::from_secs(reset)) else {
            return;
        };
        self.remaining = remaining;
        self.resets_at = resets_at;
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug)]
struct CurlResponse {
    status_code: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

#[derive(Clone)]
struct OpenSubtitlesRequestContext {
    api_base_url: String,
    token: Option<String>,
}

impl fmt::Debug for OpenSubtitlesRequestContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenSubtitlesRequestContext")
            .field("api_base_url", &self.api_base_url)
            .field("token", &self.token.as_ref().map(|_| "<private>"))
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct OnlineSubtitleStore {
    next_id: u64,
    entries: BTreeMap<String, StoredSubtitle>,
    downloaded_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredSubtitle {
    download: StoredDownload,
}

#[derive(Debug, Clone)]
enum StoredDownload {
    OpenSubtitles {
        file_id: i64,
        file_name: String,
        context: OpenSubtitlesRequestContext,
    },
    Assrt {
        subtitle_id: i64,
        token: String,
    },
    Shooter {
        url: String,
        file_name: String,
    },
}

#[derive(Debug, Clone)]
struct FoundSubtitle {
    name: String,
    left: String,
    right: String,
    download: StoredDownload,
}

impl Default for OnlineSubtitleStore {
    fn default() -> Self {
        Self {
            next_id: 1,
            entries: BTreeMap::new(),
            downloaded_paths: Vec::new(),
        }
    }
}

impl OnlineSubtitleStore {
    fn insert(&mut self, found: FoundSubtitle) -> OnlineSubtitleCandidate {
        let id = format!("online-subtitle-{}", self.next_id);
        self.next_id += 1;
        let candidate = OnlineSubtitleCandidate {
            id: id.clone(),
            name: found.name,
            left: found.left,
            right: found.right,
        };
        self.entries.insert(
            id,
            StoredSubtitle {
                download: found.download,
            },
        );
        while self.entries.len() > 200 {
            let Some(oldest) = self.entries.keys().next().cloned() else {
                break;
            };
            self.entries.remove(&oldest);
        }
        candidate
    }

    pub fn selected(&self, ids: &[String]) -> Result<Vec<StoredSubtitle>, String> {
        if ids.is_empty() {
            return Err("Select at least one subtitle".to_string());
        }
        ids.iter()
            .map(|id| {
                self.entries.get(id).cloned().ok_or_else(|| {
                    "Online subtitle selection has expired; search again".to_string()
                })
            })
            .collect()
    }

    pub fn record_downloads(&mut self, paths: &[String]) {
        self.downloaded_paths.extend(paths.iter().cloned());
        self.downloaded_paths
            .retain(|path| Path::new(path).is_file());
        if self.downloaded_paths.len() > 100 {
            let keep_from = self.downloaded_paths.len() - 100;
            self.downloaded_paths.drain(..keep_from);
        }
    }

    pub fn latest_download(&self) -> Option<String> {
        self.downloaded_paths
            .iter()
            .rev()
            .find(|path| is_downloaded_subtitle_path(Path::new(path)))
            .cloned()
    }
}

pub fn login_opensubtitles(
    username: &str,
    password: &str,
    rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<OpenSubtitlesSession, String> {
    let response = opensubtitles_curl_json(
        "POST",
        &opensubtitles_endpoint(&opensubtitles_api_base(None)?, "login"),
        &opensubtitles_headers(None),
        &[],
        Some(json!({ "username": username, "password": password }).to_string()),
        rate_limiter,
    )?;
    let token = response
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "OpenSubtitles login response did not include a token".to_string())?;
    let api_base_url = opensubtitles_api_base(response.get("base_url").and_then(Value::as_str))?;
    rate_limiter
        .lock()
        .map_err(|error| error.to_string())?
        .reset();
    Ok(OpenSubtitlesSession {
        username: username.to_string(),
        api_base_url,
        token: token.to_string(),
        expires_at: SystemTime::now()
            .checked_add(OPENSUBTITLES_SESSION_LIFETIME)
            .ok_or_else(|| "Unable to calculate OpenSubtitles session lifetime".to_string())?,
    })
}

pub fn opensubtitles_session_for_preferences(
    preferences: &PreferenceStore,
    cached_session: &Mutex<Option<OpenSubtitlesSession>>,
    rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<Option<OpenSubtitlesSession>, String> {
    {
        let mut cached_session = cached_session.lock().map_err(|error| error.to_string())?;
        if let Some(session) = cached_session.as_ref() {
            if session.is_valid() {
                return Ok(Some(session.clone()));
            }
            *cached_session = None;
        }
    }
    let username = preferences
        .values
        .get("openSubUsername")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|username| !username.is_empty());
    let Some(username) = username else {
        *cached_session.lock().map_err(|error| error.to_string())? = None;
        return Ok(None);
    };
    let Some(password) = crate::native_keychain::read_opensubtitles_password(username)
        .ok()
        .flatten()
    else {
        return Ok(None);
    };
    let session = login_opensubtitles(username, &password, rate_limiter)
        .map_err(|error| format!("OPEN_SUBTITLES_LOGIN:{error}"))?;
    *cached_session.lock().map_err(|error| error.to_string())? = Some(session.clone());
    Ok(Some(session))
}

pub fn uses_opensubtitles_provider(preferences: &PreferenceStore) -> bool {
    preferences
        .values
        .get("onlineSubProvider")
        .and_then(Value::as_str)
        .filter(|provider| !provider.is_empty())
        .unwrap_or(OPENSUBTITLES_PROVIDER)
        == OPENSUBTITLES_PROVIDER
}

fn opensubtitles_api_base(hostname: Option<&str>) -> Result<String, String> {
    let hostname = hostname
        .unwrap_or(OPENSUBTITLES_DEFAULT_API_HOST)
        .trim()
        .trim_end_matches('.');
    if hostname.is_empty() || hostname.contains('/') {
        return Err("OpenSubtitles returned an invalid API hostname".to_string());
    }
    let url = Url::parse(&format!("https://{hostname}/api/v1/"))
        .map_err(|_| "OpenSubtitles returned an invalid API hostname".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(hostname))
        || url.path() != "/api/v1/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("OpenSubtitles returned an invalid API hostname".to_string());
    }
    Ok(url.to_string())
}

fn opensubtitles_endpoint(api_base_url: &str, endpoint: &str) -> String {
    format!(
        "{}/{}",
        api_base_url.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    )
}

#[cfg(test)]
fn search(
    current_url: &str,
    media_title: &str,
    preferences: &PreferenceStore,
    store: &mut OnlineSubtitleStore,
) -> Result<OnlineSubtitleSearchResult, String> {
    search_with_opensubtitles_session(
        current_url,
        media_title,
        preferences,
        store,
        None,
        &Mutex::new(OpenSubtitlesRateLimiter::default()),
    )
}

pub fn search_with_opensubtitles_session(
    current_url: &str,
    media_title: &str,
    preferences: &PreferenceStore,
    store: &mut OnlineSubtitleStore,
    opensubtitles_session: Option<&OpenSubtitlesSession>,
    opensubtitles_rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<OnlineSubtitleSearchResult, String> {
    let provider_id = preferences
        .values
        .get("onlineSubProvider")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(OPENSUBTITLES_PROVIDER);
    let query = search_query(current_url, media_title);
    if query.is_empty() {
        return Err("Unable to determine a subtitle search title".to_string());
    }

    let (provider_name, found) = match provider_id {
        OPENSUBTITLES_PROVIDER => (
            "opensubtitles.com",
            search_opensubtitles(
                &query,
                preferred_languages(preferences),
                opensubtitles_session,
                opensubtitles_rate_limiter,
            )
            .map_err(|error| {
                tag_online_subtitle_error_if_needed(ONLINE_SUBTITLE_NETWORK_ERROR, error)
            })?,
        ),
        ASSRT_PROVIDER => (
            "assrt.net",
            search_assrt(&query, assrt_token(preferences)).map_err(|error| {
                tag_online_subtitle_error_if_needed(ONLINE_SUBTITLE_NETWORK_ERROR, error)
            })?,
        ),
        SHOOTER_PROVIDER => (
            "shooter.cn",
            search_shooter(current_url).map_err(|error| {
                tag_online_subtitle_error_if_needed(ONLINE_SUBTITLE_NETWORK_ERROR, error)
            })?,
        ),
        _ => {
            return Err(format!(
                "Online subtitle provider {provider_id} is not available; select OpenSubtitles, Assrt, or Shooter"
            ));
        }
    };

    let candidates = found
        .into_iter()
        .map(|subtitle| store.insert(subtitle))
        .collect();
    Ok(OnlineSubtitleSearchResult {
        provider_id: provider_id.to_string(),
        provider_name: provider_name.to_string(),
        query,
        candidates,
    })
}

pub(crate) fn download(
    selected: &[StoredSubtitle],
    opensubtitles_rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for (index, subtitle) in selected.iter().enumerate() {
        let files = match &subtitle.download {
            StoredDownload::OpenSubtitles {
                file_id,
                file_name,
                context,
            } => download_opensubtitles(
                *file_id,
                file_name,
                context,
                index + 1,
                opensubtitles_rate_limiter,
            )
            .map_err(|error| {
                tag_online_subtitle_error_if_needed(ONLINE_SUBTITLE_NETWORK_ERROR, error)
            })?,
            StoredDownload::Assrt { subtitle_id, token } => {
                download_assrt(*subtitle_id, token, index + 1).map_err(|error| {
                    tag_online_subtitle_error_if_needed(ONLINE_SUBTITLE_NETWORK_ERROR, error)
                })?
            }
            StoredDownload::Shooter { url, file_name } => {
                vec![
                    download_file(url, file_name, "shooter", index + 1).map_err(|error| {
                        tag_online_subtitle_error_if_needed(ONLINE_SUBTITLE_NETWORK_ERROR, error)
                    })?,
                ]
            }
        };
        paths.extend(files);
    }
    if paths.is_empty() {
        return Err(tagged_online_subtitle_error(
            ONLINE_SUBTITLE_NETWORK_ERROR,
            "The subtitle provider did not return downloadable files",
        ));
    }
    Ok(paths)
}

pub(crate) fn download_plugin_urls(
    identifier: &str,
    urls: &[String],
) -> Result<Vec<String>, String> {
    let provider = format!("plugin-{}", sanitize_file_name(identifier));
    urls.iter()
        .enumerate()
        .map(|(index, url)| {
            let file_name = plugin_subtitle_file_name(url, index + 1);
            download_file(url, &file_name, &provider, index + 1)
        })
        .collect()
}

fn plugin_subtitle_file_name(url: &str, index: usize) -> String {
    Url::parse(url)
        .ok()
        .and_then(|url| {
            Path::new(url.path())
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("plugin-subtitle-{index}.srt"))
}

fn search_opensubtitles(
    query: &str,
    languages: String,
    session: Option<&OpenSubtitlesSession>,
    rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<Vec<FoundSubtitle>, String> {
    let context = session
        .map(OpenSubtitlesSession::request_context)
        .unwrap_or(OpenSubtitlesRequestContext {
            api_base_url: opensubtitles_api_base(None)?,
            token: None,
        });
    let value = opensubtitles_curl_json(
        "GET",
        &opensubtitles_endpoint(&context.api_base_url, "subtitles"),
        &opensubtitles_headers(context.token.as_deref()),
        &[("languages", languages.as_str()), ("query", query)],
        None,
        rate_limiter,
    )?;
    let mut found = Vec::new();
    for item in value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let attributes = item.get("attributes").unwrap_or(&Value::Null);
        let file = attributes
            .get("files")
            .and_then(Value::as_array)
            .and_then(|files| files.first())
            .unwrap_or(&Value::Null);
        let Some(file_id) = file.get("file_id").and_then(Value::as_i64) else {
            continue;
        };
        let file_name = file
            .get("file_name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("subtitle.srt")
            .to_string();
        let language = attributes
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("und");
        let fps = attributes.get("fps").and_then(Value::as_f64);
        let downloads = attributes
            .get("download_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let rating = attributes
            .get("ratings")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let upload_date = attributes
            .get("upload_date")
            .and_then(Value::as_str)
            .unwrap_or("");
        let fps = fps
            .map(|value| format!(" {} fps", value.ceil() as i64))
            .unwrap_or_default();
        found.push(FoundSubtitle {
            name: file_name.clone(),
            left: format!("{language}{fps}  Downloads {downloads}  Rating {rating:.1}"),
            right: upload_date.to_string(),
            download: StoredDownload::OpenSubtitles {
                file_id,
                file_name,
                context: context.clone(),
            },
        });
    }
    Ok(found)
}

fn search_assrt(query: &str, token: String) -> Result<Vec<FoundSubtitle>, String> {
    let value = curl_json(
        "POST",
        "https://api.assrt.net/v1/sub/search",
        &[("Authorization", format!("Bearer {token}"))],
        &[("q", query)],
        None,
    )?;
    assrt_status(&value)?;
    let subtitles = value
        .get("sub")
        .and_then(|sub| sub.get("subs"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut found = Vec::new();
    for subtitle in subtitles {
        let Some(subtitle_id) = subtitle.get("id").and_then(Value::as_i64) else {
            continue;
        };
        let name = subtitle
            .get("native_name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("[No title]")
            .to_string();
        let subtitle_type = subtitle
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or("Unknown");
        let language = subtitle
            .get("lang")
            .and_then(|language| language.get("desc"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let upload_time = subtitle
            .get("upload_time")
            .and_then(Value::as_str)
            .unwrap_or("");
        found.push(FoundSubtitle {
            name,
            left: [subtitle_type, language]
                .iter()
                .filter(|value| !value.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" "),
            right: upload_time.to_string(),
            download: StoredDownload::Assrt {
                subtitle_id,
                token: token.clone(),
            },
        });
    }
    Ok(found)
}

fn search_shooter(current_url: &str) -> Result<Vec<FoundSubtitle>, String> {
    if current_url.starts_with("http://") || current_url.starts_with("https://") {
        return Err(tagged_online_subtitle_error(
            ONLINE_SUBTITLE_FILE_ERROR,
            "Shooter subtitle search requires a local media file",
        ));
    }
    let hash = shooter_file_hash(Path::new(current_url))?;
    let value = curl_json(
        "POST",
        "https://www.shooter.cn/api/subapi.php",
        &[],
        &[
            ("filehash", hash.as_str()),
            ("pathinfo", current_url),
            ("format", "json"),
        ],
        None,
    )?;
    let mut found = Vec::new();
    for (index, subtitle) in value.as_array().into_iter().flatten().enumerate() {
        let file = subtitle
            .get("Files")
            .and_then(Value::as_array)
            .and_then(|files| files.first())
            .unwrap_or(&Value::Null);
        let Some(url) = file.get("Link").and_then(Value::as_str) else {
            continue;
        };
        let extension = file
            .get("Ext")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("srt");
        let description = subtitle
            .get("Desc")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("Shooter subtitle");
        let delay = subtitle.get("Delay").and_then(Value::as_i64).unwrap_or(0);
        let file_name = format!("{}.{}", sanitize_file_name(description), extension);
        found.push(FoundSubtitle {
            name: file_name.clone(),
            left: if delay == 0 {
                "Shooter".to_string()
            } else {
                format!("Shooter  Delay {delay} ms")
            },
            right: format!("Match {}", index + 1),
            download: StoredDownload::Shooter {
                url: url.to_string(),
                file_name,
            },
        });
    }
    Ok(found)
}

fn shooter_file_hash(path: &Path) -> Result<String, String> {
    let file_size = fs::metadata(path)
        .map_err(|error| {
            tagged_online_subtitle_error(
                ONLINE_SUBTITLE_FILE_ERROR,
                format!("Unable to read media file for Shooter search: {error}"),
            )
        })?
        .len();
    if file_size < 12_288 {
        return Err(tagged_online_subtitle_error(
            ONLINE_SUBTITLE_FILE_ERROR,
            "Shooter subtitle search requires a media file of at least 12288 bytes",
        ));
    }
    let offsets = [4096, file_size / 3 * 2, file_size / 3, file_size - 8192];
    let mut file = fs::File::open(path).map_err(|error| {
        tagged_online_subtitle_error(
            ONLINE_SUBTITLE_FILE_ERROR,
            format!("Unable to open media file for Shooter search: {error}"),
        )
    })?;
    let mut chunks = Vec::with_capacity(offsets.len());
    for offset in offsets {
        file.seek(SeekFrom::Start(offset)).map_err(|error| {
            tagged_online_subtitle_error(
                ONLINE_SUBTITLE_FILE_ERROR,
                format!("Unable to seek media file for Shooter search: {error}"),
            )
        })?;
        let mut buffer = vec![0; 4096];
        file.read_exact(&mut buffer).map_err(|error| {
            tagged_online_subtitle_error(
                ONLINE_SUBTITLE_FILE_ERROR,
                format!("Unable to read media file for Shooter search: {error}"),
            )
        })?;
        let mut digest = Md5::new();
        digest.update(buffer);
        chunks.push(format!("{:x}", digest.finalize()));
    }
    Ok(chunks.join(";"))
}

fn download_opensubtitles(
    file_id: i64,
    file_name: &str,
    context: &OpenSubtitlesRequestContext,
    index: usize,
    rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<Vec<String>, String> {
    let response = opensubtitles_curl_json(
        "POST",
        &opensubtitles_endpoint(&context.api_base_url, "download"),
        &opensubtitles_headers(context.token.as_deref()),
        &[],
        Some(json!({ "file_id": file_id }).to_string()),
        rate_limiter,
    )?;
    let link = response
        .get("link")
        .and_then(Value::as_str)
        .ok_or_else(|| "OpenSubtitles download response did not include a link".to_string())?;
    let response_name = response
        .get("file_name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(file_name);
    Ok(vec![download_opensubtitles_file(
        link,
        response_name,
        index,
        context,
        rate_limiter,
    )?])
}

fn download_assrt(subtitle_id: i64, token: &str, index: usize) -> Result<Vec<String>, String> {
    let authorization = format!("Bearer {token}");
    let value = curl_json(
        "POST",
        "https://api.assrt.net/v1/sub/detail",
        &[("Authorization", authorization)],
        &[("id", &subtitle_id.to_string())],
        None,
    )?;
    assrt_status(&value)?;
    let detail = value
        .get("sub")
        .and_then(|sub| sub.get("subs"))
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| "Assrt returned no subtitle detail".to_string())?;
    let mut sources = Vec::new();
    if let Some(files) = detail.get("filelist").and_then(Value::as_array) {
        for file in files {
            if let (Some(url), Some(name)) = (
                file.get("url").and_then(Value::as_str),
                file.get("f").and_then(Value::as_str),
            ) {
                sources.push((url.to_string(), name.to_string()));
            }
        }
    } else if let (Some(url), Some(name)) = (
        detail.get("url").and_then(Value::as_str),
        detail.get("filename").and_then(Value::as_str),
    ) {
        sources.push((url.to_string(), name.to_string()));
    }
    if sources.is_empty() {
        return Err("Assrt subtitle detail did not include files".to_string());
    }
    sources
        .iter()
        .enumerate()
        .map(|(file_index, (url, name))| {
            download_file(url, name, "assrt", index * 100 + file_index + 1)
        })
        .collect()
}

fn assrt_status(value: &Value) -> Result<(), String> {
    match value.get("status").and_then(Value::as_i64) {
        Some(0) => Ok(()),
        Some(status) => Err(tagged_online_subtitle_error(
            ONLINE_SUBTITLE_NETWORK_ERROR,
            format!("Assrt returned status {status}"),
        )),
        None => Err(tagged_online_subtitle_error(
            ONLINE_SUBTITLE_NETWORK_ERROR,
            "Assrt returned an invalid response",
        )),
    }
}

fn opensubtitles_headers(token: Option<&str>) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Accept", "*/*".to_string()),
        (
            "Api-Key",
            OPENSUBTITLES_API_KEY_OBFUSCATED.chars().rev().collect(),
        ),
        ("Content-Type", "application/json".to_string()),
        ("User-Agent", "IINA v1.3.5".to_string()),
    ];
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        headers.push(("Authorization", format!("Bearer {token}")));
    }
    headers
}

fn opensubtitles_curl_json(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    form: &[(&str, &str)],
    json_body: Option<String>,
    rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<Value, String> {
    wait_for_opensubtitles_rate_limit(rate_limiter)?;
    let response = curl_response(method, url, headers, form, json_body, 20)?;
    process_opensubtitles_rate_limit_headers(rate_limiter, &response.headers)?;
    decode_opensubtitles_json_response(response)
}

fn wait_for_opensubtitles_rate_limit(
    rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<(), String> {
    let delay = rate_limiter
        .lock()
        .map_err(|error| error.to_string())?
        .delay_before_call_at(SystemTime::now());
    if !delay.is_zero() {
        thread::sleep(delay);
    }
    Ok(())
}

fn process_opensubtitles_rate_limit_headers(
    rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
    headers: &BTreeMap<String, String>,
) -> Result<(), String> {
    rate_limiter
        .lock()
        .map_err(|error| error.to_string())?
        .process_headers_at(headers, SystemTime::now());
    Ok(())
}

pub fn is_opensubtitles_invalid_token_error(error: &str) -> bool {
    error.starts_with(OPENSUBTITLES_INVALID_TOKEN_ERROR)
}

fn decode_opensubtitles_json_response(response: CurlResponse) -> Result<Value, String> {
    if response.status_code == 406
        && response_message(&response)
            .as_deref()
            .is_some_and(|message| message.eq_ignore_ascii_case("invalid token"))
    {
        return Err(format!("{OPENSUBTITLES_INVALID_TOKEN_ERROR}invalid token"));
    }
    decode_json_response(response)
}

fn curl_json(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    form: &[(&str, &str)],
    json_body: Option<String>,
) -> Result<Value, String> {
    decode_json_response(curl_response(method, url, headers, form, json_body, 20)?)
}

fn curl_response(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    form: &[(&str, &str)],
    json_body: Option<String>,
    max_time_seconds: u64,
) -> Result<CurlResponse, String> {
    let mut command = Command::new(CURL_PATH);
    command.args([
        "--silent",
        "--show-error",
        "--location",
        "--max-time",
        &max_time_seconds.to_string(),
        "--dump-header",
        "-",
    ]);
    command.arg("--request").arg(method);
    if method.eq_ignore_ascii_case("GET") {
        command.arg("--get");
    }
    for (name, value) in headers {
        command.arg("--header").arg(format!("{name}: {value}"));
    }
    for (name, value) in form {
        command
            .arg("--data-urlencode")
            .arg(format!("{name}={value}"));
    }
    if json_body.is_some() {
        command.arg("--data-binary").arg("@-");
        command.stdin(Stdio::piped());
    }
    command.arg(url);
    let output = if let Some(body) = json_body {
        let mut child = command
            .spawn()
            .map_err(|error| format!("Unable to start curl: {error}"))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "Unable to open curl request body".to_string())?
            .write_all(body.as_bytes())
            .map_err(|error| format!("Unable to write curl request body: {error}"))?;
        child
            .wait_with_output()
            .map_err(|error| format!("Unable to wait for curl: {error}"))?
    } else {
        command
            .output()
            .map_err(|error| format!("Unable to start curl: {error}"))?
    };
    if !output.status.success() {
        return Err(curl_error(&output));
    }
    parse_curl_response(&output.stdout)
        .map_err(|error| tagged_online_subtitle_error(ONLINE_SUBTITLE_NETWORK_ERROR, error))
}

fn parse_curl_response(bytes: &[u8]) -> Result<CurlResponse, String> {
    let mut offset = 0;
    let mut status_code = None;
    let mut headers = BTreeMap::new();
    while bytes
        .get(offset..)
        .is_some_and(|bytes| bytes.starts_with(b"HTTP/"))
    {
        let remaining = &bytes[offset..];
        let (header_length, separator_length) = remaining
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| (position, 4))
            .or_else(|| {
                remaining
                    .windows(2)
                    .position(|window| window == b"\n\n")
                    .map(|position| (position, 2))
            })
            .ok_or_else(|| "Subtitle provider response headers were incomplete".to_string())?;
        let header_text = String::from_utf8_lossy(&remaining[..header_length]);
        let mut lines = header_text.lines();
        let status_line = lines.next().ok_or_else(|| {
            "Subtitle provider response did not include a status line".to_string()
        })?;
        let parsed_status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u16>().ok())
            .ok_or_else(|| "Subtitle provider response included an invalid status".to_string())?;
        let mut parsed_headers = BTreeMap::new();
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            parsed_headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
        status_code = Some(parsed_status);
        headers = parsed_headers;
        offset += header_length + separator_length;
    }
    Ok(CurlResponse {
        status_code: status_code
            .ok_or_else(|| "Subtitle provider response did not include HTTP headers".to_string())?,
        headers,
        body: bytes[offset..].to_vec(),
    })
}

fn decode_json_response(response: CurlResponse) -> Result<Value, String> {
    if !(200..300).contains(&response.status_code) {
        return Err(curl_http_error(&response));
    }
    serde_json::from_slice(&response.body).map_err(|error| {
        tagged_online_subtitle_error(
            ONLINE_SUBTITLE_NETWORK_ERROR,
            format!("Subtitle provider returned invalid JSON: {error}"),
        )
    })
}

fn response_message(response: &CurlResponse) -> Option<String> {
    serde_json::from_slice::<Value>(&response.body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .or_else(|| value.get("reason"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .map(str::to_string)
        })
}

fn curl_http_error(response: &CurlResponse) -> String {
    let detail = match response_message(response) {
        Some(message) => format!(
            "Subtitle provider request failed with HTTP {}: {message}",
            response.status_code
        ),
        None => format!(
            "Subtitle provider request failed with HTTP {}",
            response.status_code
        ),
    };
    tagged_online_subtitle_error(ONLINE_SUBTITLE_NETWORK_ERROR, detail)
}

fn download_opensubtitles_file(
    url: &str,
    file_name: &str,
    index: usize,
    context: &OpenSubtitlesRequestContext,
    rate_limiter: &Mutex<OpenSubtitlesRateLimiter>,
) -> Result<String, String> {
    wait_for_opensubtitles_rate_limit(rate_limiter)?;
    let response = curl_response(
        "GET",
        url,
        &opensubtitles_headers(context.token.as_deref()),
        &[],
        None,
        30,
    )?;
    process_opensubtitles_rate_limit_headers(rate_limiter, &response.headers)?;
    if response.status_code == 406
        && response_message(&response)
            .as_deref()
            .is_some_and(|message| message.eq_ignore_ascii_case("invalid token"))
    {
        return Err(format!("{OPENSUBTITLES_INVALID_TOKEN_ERROR}invalid token"));
    }
    if !(200..300).contains(&response.status_code) {
        return Err(curl_http_error(&response));
    }
    if response.body.is_empty() {
        return Err(tagged_online_subtitle_error(
            ONLINE_SUBTITLE_FILE_ERROR,
            "Subtitle provider returned an empty file",
        ));
    }
    let destination = downloaded_subtitle_destination(file_name, "opensubtitles", index)?;
    fs::write(&destination, response.body).map_err(|error| {
        tagged_online_subtitle_error(
            ONLINE_SUBTITLE_FILE_ERROR,
            format!("Unable to save downloaded subtitle: {error}"),
        )
    })?;
    Ok(destination.display().to_string())
}

fn download_file(
    url: &str,
    file_name: &str,
    provider: &str,
    index: usize,
) -> Result<String, String> {
    let destination = downloaded_subtitle_destination(file_name, provider, index)?;
    let output = Command::new(CURL_PATH)
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "30",
        ])
        .arg("--output")
        .arg(&destination)
        .arg(url)
        .output()
        .map_err(|error| format!("Unable to start curl: {error}"))?;
    if !output.status.success() {
        let _ = fs::remove_file(&destination);
        return Err(curl_error(&output));
    }
    if fs::metadata(&destination)
        .map_err(|error| {
            tagged_online_subtitle_error(
                ONLINE_SUBTITLE_FILE_ERROR,
                format!("Unable to inspect downloaded subtitle: {error}"),
            )
        })?
        .len()
        == 0
    {
        let _ = fs::remove_file(&destination);
        return Err(tagged_online_subtitle_error(
            ONLINE_SUBTITLE_FILE_ERROR,
            "Subtitle provider returned an empty file",
        ));
    }
    Ok(destination.display().to_string())
}

fn downloaded_subtitle_destination(
    file_name: &str,
    provider: &str,
    index: usize,
) -> Result<PathBuf, String> {
    let directory = std::env::temp_dir().join("iima-online-subtitles");
    fs::create_dir_all(&directory).map_err(|error| {
        tagged_online_subtitle_error(
            ONLINE_SUBTITLE_FILE_ERROR,
            format!("Unable to create subtitle download directory: {error}"),
        )
    })?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let file_name = sanitize_file_name(file_name);
    Ok(directory.join(format!("[{index}]-{provider}-{timestamp}-{file_name}")))
}

pub fn is_downloaded_subtitle_path(path: &Path) -> bool {
    let directory = std::env::temp_dir().join("iima-online-subtitles");
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    let Ok(directory) = directory.canonicalize() else {
        return false;
    };
    path.is_file() && path.starts_with(directory)
}

fn curl_error(output: &std::process::Output) -> String {
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = if message.is_empty() {
        format!(
            "Subtitle network request failed with status {}",
            output.status
        )
    } else {
        format!("Subtitle network request failed: {message}")
    };
    tagged_online_subtitle_error(curl_error_prefix(output.status.code()), detail)
}

fn curl_error_prefix(status_code: Option<i32>) -> &'static str {
    match status_code {
        Some(7) => ONLINE_SUBTITLE_CANNOT_CONNECT_ERROR,
        Some(28) => ONLINE_SUBTITLE_TIMED_OUT_ERROR,
        Some(42) => ONLINE_SUBTITLE_CANCELED_ERROR,
        _ => ONLINE_SUBTITLE_NETWORK_ERROR,
    }
}

fn tagged_online_subtitle_error(prefix: &str, detail: impl AsRef<str>) -> String {
    format!("{prefix}{}", detail.as_ref())
}

fn tag_online_subtitle_error_if_needed(prefix: &str, error: impl AsRef<str>) -> String {
    let error = error.as_ref();
    if [
        OPENSUBTITLES_INVALID_TOKEN_ERROR,
        ONLINE_SUBTITLE_CANNOT_CONNECT_ERROR,
        ONLINE_SUBTITLE_NETWORK_ERROR,
        ONLINE_SUBTITLE_TIMED_OUT_ERROR,
        ONLINE_SUBTITLE_FILE_ERROR,
        ONLINE_SUBTITLE_CANCELED_ERROR,
    ]
    .iter()
    .any(|known_prefix| error.starts_with(known_prefix))
    {
        error.to_string()
    } else {
        tagged_online_subtitle_error(prefix, error)
    }
}

fn preferred_languages(preferences: &PreferenceStore) -> String {
    preferences
        .values
        .get("subLang")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "en".to_string())
}

fn assrt_token(preferences: &PreferenceStore) -> String {
    preferences
        .values
        .get("assrtToken")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 32)
        .unwrap_or(ASSRT_FALLBACK_TOKEN)
        .to_string()
}

fn search_query(current_url: &str, media_title: &str) -> String {
    let local_title = Path::new(current_url)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(title) = local_title {
        return title.to_string();
    }
    Url::parse(current_url)
        .ok()
        .and_then(|url| {
            PathBuf::from(url.path())
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| media_title.trim().to_string())
}

fn sanitize_file_name(file_name: &str) -> String {
    let leaf_name = Path::new(file_name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name);
    let sanitized = leaf_name
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' | ' ' => character,
            _ => '_',
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if sanitized.is_empty() {
        "subtitle.srt".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_iina_default_provider_and_language() {
        let preferences = PreferenceStore::default();

        assert_eq!(preferred_languages(&preferences), "en");
        assert_eq!(assrt_token(&preferences), ASSRT_FALLBACK_TOKEN);
        assert_eq!(
            preferences.values["openSubUsername"],
            Value::String(String::new())
        );
    }

    #[test]
    fn classifies_transport_failures_for_iina_osd_messages() {
        assert_eq!(
            curl_error_prefix(Some(7)),
            ONLINE_SUBTITLE_CANNOT_CONNECT_ERROR
        );
        assert_eq!(curl_error_prefix(Some(28)), ONLINE_SUBTITLE_TIMED_OUT_ERROR);
        assert_eq!(curl_error_prefix(Some(42)), ONLINE_SUBTITLE_CANCELED_ERROR);
        assert_eq!(curl_error_prefix(Some(6)), ONLINE_SUBTITLE_NETWORK_ERROR);
        assert_eq!(curl_error_prefix(None), ONLINE_SUBTITLE_NETWORK_ERROR);
    }

    #[test]
    fn tags_file_and_network_errors_without_losing_diagnostics() {
        assert_eq!(
            tagged_online_subtitle_error(ONLINE_SUBTITLE_FILE_ERROR, "disk failed"),
            "ONLINE_SUBTITLE_FILE_ERROR:disk failed"
        );
        assert_eq!(
            tagged_online_subtitle_error(ONLINE_SUBTITLE_NETWORK_ERROR, "bad response"),
            "ONLINE_SUBTITLE_NETWORK_ERROR:bad response"
        );
        assert_eq!(
            tag_online_subtitle_error_if_needed(
                ONLINE_SUBTITLE_NETWORK_ERROR,
                "ONLINE_SUBTITLE_FILE_ERROR:disk failed"
            ),
            "ONLINE_SUBTITLE_FILE_ERROR:disk failed"
        );
        assert_eq!(
            tag_online_subtitle_error_if_needed(ONLINE_SUBTITLE_NETWORK_ERROR, "provider failed"),
            "ONLINE_SUBTITLE_NETWORK_ERROR:provider failed"
        );
    }

    #[test]
    fn forms_and_validates_opensubtitles_api_urls() {
        assert_eq!(
            opensubtitles_api_base(None).unwrap(),
            "https://api.opensubtitles.com/api/v1/"
        );
        assert_eq!(
            opensubtitles_endpoint("https://vip.opensubtitles.com/api/v1/", "/download"),
            "https://vip.opensubtitles.com/api/v1/download"
        );
        for hostname in [
            "",
            "https://evil.example",
            "evil.example/path",
            "evil.example?x=1",
            "evil.example:443",
        ] {
            assert!(
                opensubtitles_api_base(Some(hostname)).is_err(),
                "accepted {hostname}"
            );
        }
    }

    #[test]
    fn adds_authorization_only_for_authenticated_opensubtitles_requests() {
        let anonymous = opensubtitles_headers(None);
        assert!(!anonymous.iter().any(|(name, _)| *name == "Authorization"));
        let authenticated = opensubtitles_headers(Some("secret-token"));
        assert!(authenticated
            .iter()
            .any(|(name, value)| *name == "Authorization" && value == "Bearer secret-token"));
    }

    #[test]
    fn reuses_valid_cached_opensubtitles_session_without_keychain_access() {
        let session = OpenSubtitlesSession {
            username: "tester".to_string(),
            api_base_url: opensubtitles_api_base(None).unwrap(),
            token: "secret-token".to_string(),
            expires_at: SystemTime::now() + Duration::from_secs(60),
        };
        let cached = Mutex::new(Some(session));
        let rate_limiter = Mutex::new(OpenSubtitlesRateLimiter::default());
        let restored = opensubtitles_session_for_preferences(
            &PreferenceStore::default(),
            &cached,
            &rate_limiter,
        )
        .unwrap()
        .unwrap();

        assert_eq!(restored.username, "tester");
        assert!(!format!("{restored:?}").contains("secret-token"));
    }

    #[test]
    fn discards_expired_cached_opensubtitles_session() {
        let session = OpenSubtitlesSession {
            username: "tester".to_string(),
            api_base_url: opensubtitles_api_base(None).unwrap(),
            token: "expired-secret-token".to_string(),
            expires_at: SystemTime::UNIX_EPOCH,
        };
        let cached = Mutex::new(Some(session));
        let rate_limiter = Mutex::new(OpenSubtitlesRateLimiter::default());

        let restored = opensubtitles_session_for_preferences(
            &PreferenceStore::default(),
            &cached,
            &rate_limiter,
        )
        .unwrap();

        assert!(restored.is_none());
        assert!(cached.lock().unwrap().is_none());
    }

    #[test]
    fn obeys_opensubtitles_rate_limit_window_and_ignores_malformed_headers() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        let mut limiter = OpenSubtitlesRateLimiter::default();
        limiter.process_headers_at(
            &BTreeMap::from([
                ("ratelimit-remaining".to_string(), "1".to_string()),
                ("ratelimit-reset".to_string(), "3".to_string()),
            ]),
            now,
        );

        assert_eq!(limiter.delay_before_call_at(now), Duration::ZERO);
        assert_eq!(limiter.delay_before_call_at(now), Duration::from_secs(3));

        limiter.process_headers_at(
            &BTreeMap::from([
                ("ratelimit-remaining".to_string(), "invalid".to_string()),
                ("ratelimit-reset".to_string(), "2".to_string()),
            ]),
            now,
        );
        assert_eq!(limiter.delay_before_call_at(now), Duration::from_secs(3));
        assert_eq!(
            limiter.delay_before_call_at(now + Duration::from_secs(3)),
            Duration::ZERO
        );
    }

    #[test]
    fn parses_final_curl_headers_after_redirects() {
        let response = parse_curl_response(
            b"HTTP/1.1 302 Found\r\nLocation: https://api.example/final\r\n\r\nHTTP/2 200\r\nRateLimit-Remaining: 4\r\nRateLimit-Reset: 1\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}",
        )
        .unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.headers["ratelimit-remaining"], "4");
        assert_eq!(response.headers["ratelimit-reset"], "1");
        assert_eq!(response.body, br#"{"ok":true}"#);
    }

    #[test]
    fn marks_server_rejected_opensubtitles_tokens() {
        let error = decode_opensubtitles_json_response(CurlResponse {
            status_code: 406,
            headers: BTreeMap::new(),
            body: br#"{"message":"invalid token"}"#.to_vec(),
        })
        .unwrap_err();

        assert!(is_opensubtitles_invalid_token_error(&error));
    }

    #[test]
    fn derives_search_query_from_local_or_remote_media() {
        assert_eq!(search_query("/Movies/The Film.mkv", "Ignored"), "The Film");
        assert_eq!(
            search_query(
                "https://example.com/media/Remote%20Film.mp4?token=1",
                "Fallback"
            ),
            "Remote%20Film"
        );
        assert_eq!(search_query("", "Fallback"), "Fallback");
    }

    #[test]
    fn sanitizes_downloaded_file_names() {
        assert_eq!(sanitize_file_name("../../sub:title?.srt"), "sub_title_.srt");
        assert_eq!(sanitize_file_name(".."), "subtitle.srt");
    }

    #[test]
    fn creates_four_shooter_md5_chunks_for_local_media() {
        let path =
            std::env::temp_dir().join(format!("iima-shooter-hash-{}.bin", std::process::id()));
        let bytes = (0..16_384)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(&path, bytes).unwrap();

        let hash = shooter_file_hash(&path).unwrap();
        assert_eq!(hash.split(';').count(), 4);
        assert!(hash.split(';').all(|chunk| chunk.len() == 32));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_too_small_shooter_media() {
        let path =
            std::env::temp_dir().join(format!("iima-shooter-small-{}.bin", std::process::id()));
        fs::write(&path, [0_u8; 32]).unwrap();

        assert!(shooter_file_hash(&path).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn stores_opaque_candidate_handles() {
        let mut store = OnlineSubtitleStore::default();
        let candidate = store.insert(FoundSubtitle {
            name: "sample.srt".to_string(),
            left: "EN".to_string(),
            right: "today".to_string(),
            download: StoredDownload::OpenSubtitles {
                file_id: 42,
                file_name: "sample.srt".to_string(),
                context: OpenSubtitlesRequestContext {
                    api_base_url: opensubtitles_api_base(None).unwrap(),
                    token: None,
                },
            },
        });

        let selected = store.selected(&[candidate.id]).unwrap();
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn remembers_only_existing_downloads_inside_the_online_subtitle_directory() {
        let directory = std::env::temp_dir().join("iima-online-subtitles");
        fs::create_dir_all(&directory).unwrap();
        let file = directory.join(format!("iima-online-subtitle-{}.srt", std::process::id()));
        fs::write(&file, "1\n00:00:00,000 --> 00:00:01,000\nFixture\n").unwrap();
        let mut store = OnlineSubtitleStore::default();
        store.record_downloads(&[
            file.display().to_string(),
            "/tmp/not-online.srt".to_string(),
        ]);

        assert_eq!(store.latest_download().as_deref(), file.to_str());
        let _ = fs::remove_file(file);
    }

    #[test]
    #[ignore = "requires live OpenSubtitles and Assrt service access"]
    fn live_provider_contracts_return_search_results() {
        let opensubtitles_preferences = PreferenceStore::default();
        let mut opensubtitles_store = OnlineSubtitleStore::default();
        let opensubtitles = search(
            "/tmp/IINA.mkv",
            "IINA",
            &opensubtitles_preferences,
            &mut opensubtitles_store,
        )
        .expect("OpenSubtitles search should complete");
        assert_eq!(opensubtitles.provider_id, OPENSUBTITLES_PROVIDER);

        let mut assrt_preferences = PreferenceStore::default();
        assrt_preferences.values.insert(
            "onlineSubProvider".to_string(),
            Value::String(ASSRT_PROVIDER.to_string()),
        );
        let mut assrt_store = OnlineSubtitleStore::default();
        let assrt = search(
            "/tmp/IINA.mkv",
            "IINA",
            &assrt_preferences,
            &mut assrt_store,
        )
        .expect("Assrt search should complete");
        assert_eq!(assrt.provider_id, ASSRT_PROVIDER);
    }

    #[test]
    #[ignore = "downloads one live OpenSubtitles file and consumes provider quota"]
    fn live_opensubtitles_downloads_a_nonempty_temp_file() {
        let preferences = PreferenceStore::default();
        let mut store = OnlineSubtitleStore::default();
        let result = search("/tmp/IINA.mkv", "IINA", &preferences, &mut store)
            .expect("OpenSubtitles search should complete");
        let candidate = result
            .candidates
            .first()
            .expect("OpenSubtitles should return at least one candidate");
        let selected = store.selected(std::slice::from_ref(&candidate.id)).unwrap();
        let rate_limiter = Mutex::new(OpenSubtitlesRateLimiter::default());
        let files =
            download(&selected, &rate_limiter).expect("OpenSubtitles download should complete");
        assert!(files.iter().all(|path| {
            fs::metadata(path)
                .map(|metadata| metadata.len() > 0)
                .unwrap_or(false)
        }));
        for file in files {
            let _ = fs::remove_file(file);
        }
    }

    #[test]
    #[ignore = "requires IIMA_SHOOTER_TEST_MEDIA and live Shooter service access"]
    fn live_shooter_contract_accepts_a_local_media_hash() {
        let media = std::env::var("IIMA_SHOOTER_TEST_MEDIA")
            .expect("IIMA_SHOOTER_TEST_MEDIA must point to a local media file");
        let mut preferences = PreferenceStore::default();
        preferences.values.insert(
            "onlineSubProvider".to_string(),
            Value::String(SHOOTER_PROVIDER.to_string()),
        );
        let mut store = OnlineSubtitleStore::default();
        let result = search(&media, "Shooter test", &preferences, &mut store)
            .expect("Shooter search should return parseable JSON");
        assert_eq!(result.provider_id, SHOOTER_PROVIDER);
    }
}
