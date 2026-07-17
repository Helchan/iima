use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeMenuCatalog {
    default_locale: String,
    locales: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    contexts: HashMap<String, HashMap<String, String>>,
}

static CATALOG: OnceLock<NativeMenuCatalog> = OnceLock::new();
static ACTIVE_LOCALE: OnceLock<String> = OnceLock::new();

fn catalog() -> &'static NativeMenuCatalog {
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("native-menu-locales.json"))
            .expect("generated native menu localization catalog must be valid")
    })
}

fn normalize_locale(locale: &str) -> Option<String> {
    let parts = locale
        .replace('_', "-")
        .split('-')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("-"))
    }
}

fn resolve_locale(preferred: &[String], supported: &[String], default_locale: &str) -> String {
    let supported_by_lowercase = supported
        .iter()
        .map(|locale| (locale.to_ascii_lowercase(), locale.as_str()))
        .collect::<HashMap<_, _>>();
    for preferred_locale in preferred {
        let Some(locale) = normalize_locale(preferred_locale) else {
            continue;
        };
        let lower = locale.to_ascii_lowercase();
        if let Some(supported) = supported_by_lowercase.get(&lower) {
            return (*supported).to_string();
        }

        let parts = lower.split('-').collect::<Vec<_>>();
        for length in (2..parts.len()).rev() {
            let parent = parts[..length].join("-");
            if let Some(supported) = supported_by_lowercase.get(&parent) {
                return (*supported).to_string();
            }
        }

        let language = parts[0];
        if language == "zh" {
            let traditional = parts
                .iter()
                .skip(1)
                .any(|part| matches!(*part, "hant" | "tw" | "hk" | "mo"));
            let chinese = if traditional { "zh-hant" } else { "zh-hans" };
            if let Some(supported) = supported_by_lowercase.get(chinese) {
                return (*supported).to_string();
            }
        }
        if let Some(supported) = supported_by_lowercase.get(language) {
            return (*supported).to_string();
        }
    }
    supported_by_lowercase
        .get(&default_locale.to_ascii_lowercase())
        .copied()
        .unwrap_or(default_locale)
        .to_string()
}

#[cfg(all(target_os = "macos", not(test)))]
fn preferred_languages() -> Vec<String> {
    use std::ffi::{c_char, CStr};

    unsafe extern "C" {
        fn iima_native_preferred_languages_json() -> *mut c_char;
        fn iima_native_free_localization_string(value: *mut c_char);
    }

    let raw = unsafe { iima_native_preferred_languages_json() };
    if raw.is_null() {
        return vec!["en".to_string()];
    }
    let json = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    unsafe { iima_native_free_localization_string(raw) };
    serde_json::from_str(&json).unwrap_or_else(|_| vec!["en".to_string()])
}

#[cfg(any(not(target_os = "macos"), test))]
fn preferred_languages() -> Vec<String> {
    vec!["en".to_string()]
}

fn active_locale() -> &'static str {
    ACTIVE_LOCALE.get_or_init(|| {
        let catalog = catalog();
        let mut supported = catalog.locales.keys().cloned().collect::<Vec<_>>();
        supported.push(catalog.default_locale.clone());
        resolve_locale(&preferred_languages(), &supported, &catalog.default_locale)
    })
}

fn menu_title_for_locale(locale: &str, source: &str) -> String {
    catalog()
        .locales
        .get(locale)
        .and_then(|translations| translations.get(source))
        .cloned()
        .unwrap_or_else(|| source.to_string())
}

fn context_identifier(table: &str, key: &str) -> Option<String> {
    let table = table.trim();
    let key = key.trim();
    if table.is_empty() || key.is_empty() {
        return None;
    }
    let filename = if table.ends_with(".strings") {
        table.to_string()
    } else {
        format!("{table}.strings")
    };
    Some(format!("{filename}:{key}"))
}

fn menu_title_key_for_locale(locale: &str, table: &str, key: &str, source: &str) -> String {
    context_identifier(table, key)
        .and_then(|context| {
            catalog()
                .contexts
                .get(locale)
                .and_then(|translations| translations.get(&context))
                .cloned()
        })
        .unwrap_or_else(|| menu_title_for_locale(locale, source))
}

pub fn menu_title(source: &str) -> String {
    menu_title_for_locale(active_locale(), source)
}

pub fn menu_title_key(table: &str, key: &str, source: &str) -> String {
    menu_title_key_for_locale(active_locale(), table, key, source)
}

fn format_number_template(template: &str, value: f64) -> String {
    let bytes = template.as_bytes();
    let mut result = String::with_capacity(template.len() + 8);
    let mut cursor = 0;
    while let Some(relative) = template[cursor..].find('%') {
        let percent = cursor + relative;
        result.push_str(&template[cursor..percent]);
        if bytes.get(percent + 1) == Some(&b'%') {
            result.push('%');
            cursor = percent + 2;
            continue;
        }

        let mut end = percent + 1;
        while let Some(byte) = bytes.get(end) {
            if matches!(*byte, b'd' | b'i' | b'f') {
                let token = &template[percent..=end];
                if *byte == b'd' || *byte == b'i' {
                    result.push_str(&(value.round() as i64).to_string());
                } else {
                    let precision = token
                        .split_once('.')
                        .and_then(|(_, suffix)| suffix.strip_suffix('f'))
                        .and_then(|digits| digits.parse::<usize>().ok())
                        .unwrap_or(6);
                    result.push_str(&format!("{value:.precision$}"));
                }
                cursor = end + 1;
                break;
            }
            if !matches!(*byte, b'0'..=b'9' | b'.' | b'-' | b'+' | b' ' | b'#') {
                result.push('%');
                cursor = percent + 1;
                break;
            }
            end += 1;
        }
        if end >= bytes.len() {
            result.push_str(&template[percent..]);
            cursor = template.len();
        }
    }
    result.push_str(&template[cursor..]);
    result
}

fn menu_number_for_locale(locale: &str, source: &str, value: f64) -> String {
    format_number_template(&menu_title_for_locale(locale, source), value)
}

fn menu_number_key_for_locale(
    locale: &str,
    table: &str,
    key: &str,
    source: &str,
    value: f64,
) -> String {
    format_number_template(
        &menu_title_key_for_locale(locale, table, key, source),
        value,
    )
}

pub fn menu_number(source: &str, value: f64) -> String {
    menu_number_for_locale(active_locale(), source, value)
}

pub fn menu_number_key(table: &str, key: &str, source: &str, value: f64) -> String {
    menu_number_key_for_locale(active_locale(), table, key, source, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supported() -> Vec<String> {
        let catalog = catalog();
        let mut supported = catalog.locales.keys().cloned().collect::<Vec<_>>();
        supported.push(catalog.default_locale.clone());
        supported
    }

    #[test]
    fn native_locale_resolution_matches_frontend_fallbacks() {
        let supported = supported();
        assert_eq!(
            resolve_locale(&["zh-CN".to_string()], &supported, "en"),
            "zh-Hans"
        );
        assert_eq!(
            resolve_locale(&["zh-HK".to_string()], &supported, "en"),
            "zh-Hant"
        );
        assert_eq!(
            resolve_locale(&["sr-Latn-RS".to_string()], &supported, "en"),
            "sr-Latn"
        );
        assert_eq!(
            resolve_locale(&["en-AU".to_string()], &supported, "en"),
            "en"
        );
    }

    #[test]
    fn native_menu_catalog_translates_static_and_formatted_titles() {
        assert_eq!(menu_title_for_locale("zh-Hans", "File"), "文件");
        assert_eq!(
            menu_title_for_locale("zh-Hans", "Choose Media Files"),
            "选择媒体文件"
        );
        assert_eq!(
            menu_title_for_locale("zh-Hans", "Check for Updates..."),
            "检查更新…"
        );
        assert_eq!(
            menu_title_for_locale("zh-Hans", "Choose a Font"),
            "选择字体"
        );
        assert_eq!(
            menu_title_for_locale("zh-Hans", "Save Downloaded Subtitle"),
            "保存下载的字幕"
        );
        assert_eq!(
            menu_number_for_locale("zh-Hans", "Speed: %.2fx", 1.25),
            "速度: 1.25x"
        );
        assert_eq!(
            menu_number_for_locale("zh-Hans", "Volume: %d", 87.6),
            "音量: 88"
        );
    }

    #[test]
    fn native_catalog_uses_exact_table_and_key_before_source_fallback() {
        assert_eq!(
            menu_title_key_for_locale(
                "zh-Hans",
                "InitialWindowController",
                "KWZ-BM-GBN.title",
                "Resume"
            ),
            "继续播放"
        );
        assert_eq!(
            menu_title_key_for_locale("zh-Hans", "Localizable", "osd.resume", "Resume"),
            "继续"
        );
        assert_eq!(
            menu_number_key_for_locale("zh-Hans", "Localizable", "osd.speed", "Speed: %.2fx", 1.25),
            "速度：1.25x"
        );
        assert_eq!(
            menu_number_key_for_locale(
                "zh-Hans",
                "Localizable.strings",
                "menu.speed",
                "Speed: %.2fx",
                1.25
            ),
            "速度: 1.25x"
        );
        assert_eq!(
            menu_title_key_for_locale("zh-Hans", "Missing", "missing", "File"),
            "文件"
        );
    }
}
