use serde_json::Value;

const PLIST_TAG_CONTAINER: &str = "__iimaUserDefaultsPlistValue";

/// A deliberately small color codec for values created by this port. IINA's
/// original value is an `NSArchiver` Data blob; those blobs are detected and
/// preserved byte-for-byte by the preferences mirror instead of being guessed
/// at or silently converted into a different color.
pub fn mpv_color(value: &Value) -> Option<String> {
    let raw = value.as_str()?;
    if let Some(hex) = raw.strip_prefix('#') {
        return hex_color(hex);
    }
    slash_color(raw)
}

pub fn is_preserved_iina_archive(value: &Value) -> bool {
    let Some(outer) = value.as_object().filter(|outer| outer.len() == 1) else {
        return false;
    };
    let Some(tagged) = outer
        .get(PLIST_TAG_CONTAINER)
        .and_then(Value::as_object)
        .filter(|tagged| tagged.len() == 2)
    else {
        return false;
    };
    tagged.get("type").and_then(Value::as_str) == Some("data")
        && tagged
            .get("value")
            .and_then(Value::as_str)
            .is_some_and(|hex| {
                !hex.is_empty()
                    && hex.len() % 2 == 0
                    && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
}

pub fn validate(value: &Value) -> Result<(), String> {
    if mpv_color(value).is_some() || is_preserved_iina_archive(value) {
        Ok(())
    } else {
        Err("must be #RRGGBB, #RRGGBBAA, r/g/b[/a], or preserved IINA NSColor Data".into())
    }
}

fn hex_color(hex: &str) -> Option<String> {
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut components = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    if components.len() == 3 {
        components.push(255);
    }
    Some(
        components
            .into_iter()
            .map(format_component)
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn slash_color(raw: &str) -> Option<String> {
    let components = raw
        .split('/')
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if !matches!(components.len(), 3 | 4)
        || components
            .iter()
            .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
    {
        return None;
    }
    let mut components = components;
    if components.len() == 3 {
        components.push(1.0);
    }
    Some(
        components
            .into_iter()
            .map(format_unit_component)
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn format_component(component: u8) -> String {
    match component {
        0 => "0".into(),
        255 => "1".into(),
        component => format_unit_component(f64::from(component) / 255.0),
    }
}

fn format_unit_component(component: f64) -> String {
    let mut value = format!("{component:.6}");
    while value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn css_and_mpv_values_normalize_without_losing_alpha() {
        assert_eq!(mpv_color(&json!("#ffffffff")).as_deref(), Some("1/1/1/1"));
        assert_eq!(mpv_color(&json!("#00000000")).as_deref(), Some("0/0/0/0"));
        assert_eq!(
            mpv_color(&json!("#80402080")).as_deref(),
            Some("0.501961/0.25098/0.12549/0.501961")
        );
        assert_eq!(
            mpv_color(&json!("0.5/0.25/0/1")).as_deref(),
            Some("0.5/0.25/0/1")
        );
    }

    #[test]
    fn imported_nscolor_data_is_an_explicit_preservation_boundary() {
        let archive = json!({
            "__iimaUserDefaultsPlistValue": {
                "type": "data",
                "value": "0001feff"
            }
        });
        assert!(is_preserved_iina_archive(&archive));
        assert!(validate(&archive).is_ok());
        assert_eq!(mpv_color(&archive), None);
        assert!(!is_preserved_iina_archive(&json!({
            "__iimaUserDefaultsPlistValue": {"type": "date", "value": "2024-01-01T00:00:00Z"}
        })));
    }

    #[test]
    fn invalid_or_out_of_range_values_are_rejected() {
        for value in [json!("#ffff"), json!("2/0/0/1"), json!(-1), json!({})] {
            assert!(validate(&value).is_err(), "{value}");
        }
    }
}
