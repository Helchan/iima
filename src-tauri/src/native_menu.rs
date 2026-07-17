#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMenuKeyEquivalent {
    menu_title: String,
    item_title: Option<String>,
    submenu_title: Option<String>,
    item_index: Option<usize>,
    item_path: Option<Vec<usize>>,
    key_equivalent: String,
    modifier_mask: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMenuResponderAction {
    menu_title: String,
    submenu_title: Option<String>,
    item_title: String,
    selector: String,
    key_equivalent: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMenuItemState {
    menu_title: String,
    item_path: Vec<usize>,
    selected: bool,
}

impl NativeMenuItemState {
    pub fn path(menu_title: String, item_path: Vec<usize>, selected: bool) -> Self {
        Self {
            menu_title,
            item_path,
            selected,
        }
    }
}

impl NativeMenuResponderAction {
    pub fn item(
        menu_title: String,
        item_title: String,
        selector: impl Into<String>,
        key_equivalent: impl Into<String>,
    ) -> Self {
        Self {
            menu_title,
            submenu_title: None,
            item_title,
            selector: selector.into(),
            key_equivalent: key_equivalent.into(),
        }
    }

    pub fn submenu_item(
        menu_title: String,
        submenu_title: String,
        item_title: String,
        selector: impl Into<String>,
    ) -> Self {
        Self {
            menu_title,
            submenu_title: Some(submenu_title),
            item_title,
            selector: selector.into(),
            key_equivalent: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMenuVisibility {
    menu_title: String,
    item_title: String,
    hidden: bool,
}

impl NativeMenuVisibility {
    pub fn hidden(menu_title: String, item_title: String) -> Self {
        Self {
            menu_title,
            item_title,
            hidden: true,
        }
    }
}

impl NativeMenuKeyEquivalent {
    pub fn item(
        menu_title: String,
        item_title: String,
        key_equivalent: String,
        modifier_mask: u32,
    ) -> Self {
        Self {
            menu_title,
            item_title: Some(item_title),
            submenu_title: None,
            item_index: None,
            item_path: None,
            key_equivalent,
            modifier_mask,
        }
    }

    pub fn submenu_index(
        menu_title: String,
        submenu_title: String,
        item_index: usize,
        key_equivalent: String,
        modifier_mask: u32,
    ) -> Self {
        Self {
            menu_title,
            item_title: None,
            submenu_title: Some(submenu_title),
            item_index: Some(item_index),
            item_path: None,
            key_equivalent,
            modifier_mask,
        }
    }

    pub fn path(
        menu_title: String,
        item_path: Vec<usize>,
        key_equivalent: String,
        modifier_mask: u32,
    ) -> Self {
        Self {
            menu_title,
            item_title: None,
            submenu_title: None,
            item_index: None,
            item_path: Some(item_path),
            key_equivalent,
            modifier_mask,
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::NativeMenuKeyEquivalent;
    use std::ffi::{c_char, c_int, CString};

    unsafe extern "C" {
        fn iima_native_set_menu_item_key_equivalent(
            menu_title: *const c_char,
            item_title: *const c_char,
            key_equivalent: *const c_char,
            modifier_mask: u32,
        ) -> c_int;
        fn iima_native_set_submenu_item_key_equivalent(
            menu_title: *const c_char,
            submenu_title: *const c_char,
            item_index: usize,
            key_equivalent: *const c_char,
            modifier_mask: u32,
        ) -> c_int;
        fn iima_native_set_menu_item_key_equivalent_at_path(
            menu_title: *const c_char,
            item_path: *const usize,
            item_path_length: usize,
            key_equivalent: *const c_char,
            modifier_mask: u32,
        ) -> c_int;
        fn iima_native_plugin_developer_tool_available() -> c_int;
        fn iima_native_set_menu_item_state_at_path(
            menu_title: *const c_char,
            item_path: *const usize,
            item_path_length: usize,
            selected: c_int,
        ) -> c_int;
        fn iima_native_mark_menu_item_alternate(
            menu_title: *const c_char,
            item_title: *const c_char,
            require_option_accelerator: c_int,
        ) -> c_int;
        fn iima_native_set_menu_item_responder_action(
            menu_title: *const c_char,
            submenu_title: *const c_char,
            item_title: *const c_char,
            selector: *const c_char,
            key_equivalent: *const c_char,
        ) -> c_int;
        fn iima_native_set_menu_item_hidden(
            menu_title: *const c_char,
            item_title: *const c_char,
            hidden: c_int,
        ) -> c_int;
    }

    pub fn configure_key_equivalents(items: &[NativeMenuKeyEquivalent]) -> Result<(), String> {
        for item in items {
            let menu_title = CString::new(item.menu_title.as_str())
                .map_err(|_| "native menu title contains a NUL byte".to_string())?;
            let key_equivalent = CString::new(item.key_equivalent.as_str())
                .map_err(|_| "native menu key equivalent contains a NUL byte".to_string())?;
            let configured = if let Some(item_path) = &item.item_path {
                if item_path.is_empty() {
                    0
                } else {
                    unsafe {
                        iima_native_set_menu_item_key_equivalent_at_path(
                            menu_title.as_ptr(),
                            item_path.as_ptr(),
                            item_path.len(),
                            key_equivalent.as_ptr(),
                            item.modifier_mask,
                        )
                    }
                }
            } else if let (Some(submenu_title), Some(item_index)) =
                (&item.submenu_title, item.item_index)
            {
                let submenu_title = CString::new(submenu_title.as_str())
                    .map_err(|_| "native submenu title contains a NUL byte".to_string())?;
                unsafe {
                    iima_native_set_submenu_item_key_equivalent(
                        menu_title.as_ptr(),
                        submenu_title.as_ptr(),
                        item_index,
                        key_equivalent.as_ptr(),
                        item.modifier_mask,
                    )
                }
            } else if let Some(item_title) = &item.item_title {
                let item_title = CString::new(item_title.as_str())
                    .map_err(|_| "native menu item title contains a NUL byte".to_string())?;
                unsafe {
                    iima_native_set_menu_item_key_equivalent(
                        menu_title.as_ptr(),
                        item_title.as_ptr(),
                        key_equivalent.as_ptr(),
                        item.modifier_mask,
                    )
                }
            } else {
                0
            };
            if configured == 0 {
                return Err(format!(
                    "failed to configure key equivalent under {}",
                    item.menu_title
                ));
            }
        }
        Ok(())
    }

    pub fn configure_alternate_items(items: &[(String, String, bool)]) -> Result<(), String> {
        for (menu_title, item_title, require_option_accelerator) in items {
            let menu_title = CString::new(menu_title.as_str())
                .map_err(|_| "native menu title contains a NUL byte".to_string())?;
            let item_title = CString::new(item_title.as_str())
                .map_err(|_| "native menu item title contains a NUL byte".to_string())?;
            let configured = unsafe {
                iima_native_mark_menu_item_alternate(
                    menu_title.as_ptr(),
                    item_title.as_ptr(),
                    i32::from(*require_option_accelerator),
                )
            };
            if configured == 0 {
                return Err(format!(
                    "failed to configure alternate menu item: {} > {}",
                    menu_title.to_string_lossy(),
                    item_title.to_string_lossy()
                ));
            }
        }
        Ok(())
    }

    pub fn configure_item_states(items: &[super::NativeMenuItemState]) -> Result<(), String> {
        for item in items {
            let menu_title = CString::new(item.menu_title.as_str())
                .map_err(|_| "native menu title contains a NUL byte".to_string())?;
            if item.item_path.is_empty() {
                return Err("native menu state path is empty".to_string());
            }
            let configured = unsafe {
                iima_native_set_menu_item_state_at_path(
                    menu_title.as_ptr(),
                    item.item_path.as_ptr(),
                    item.item_path.len(),
                    i32::from(item.selected),
                )
            };
            if configured == 0 {
                return Err(format!(
                    "failed to configure menu state under {}",
                    item.menu_title
                ));
            }
        }
        Ok(())
    }

    pub fn configure_responder_actions(
        items: &[super::NativeMenuResponderAction],
    ) -> Result<(), String> {
        for item in items {
            let menu_title = CString::new(item.menu_title.as_str())
                .map_err(|_| "native menu title contains a NUL byte".to_string())?;
            let submenu_title = CString::new(item.submenu_title.as_deref().unwrap_or_default())
                .map_err(|_| "native submenu title contains a NUL byte".to_string())?;
            let item_title = CString::new(item.item_title.as_str())
                .map_err(|_| "native menu item title contains a NUL byte".to_string())?;
            let selector = CString::new(item.selector.as_str())
                .map_err(|_| "native menu selector contains a NUL byte".to_string())?;
            let key_equivalent = CString::new(item.key_equivalent.as_str())
                .map_err(|_| "native menu key equivalent contains a NUL byte".to_string())?;
            let configured = unsafe {
                iima_native_set_menu_item_responder_action(
                    menu_title.as_ptr(),
                    submenu_title.as_ptr(),
                    item_title.as_ptr(),
                    selector.as_ptr(),
                    key_equivalent.as_ptr(),
                )
            };
            if configured == 0 {
                return Err(format!(
                    "failed to configure responder action: {} > {}",
                    item.menu_title, item.item_title
                ));
            }
        }
        Ok(())
    }

    pub fn configure_visibility(items: &[super::NativeMenuVisibility]) -> Result<(), String> {
        for item in items {
            let menu_title = CString::new(item.menu_title.as_str())
                .map_err(|_| "native menu title contains a NUL byte".to_string())?;
            let item_title = CString::new(item.item_title.as_str())
                .map_err(|_| "native menu item title contains a NUL byte".to_string())?;
            let configured = unsafe {
                iima_native_set_menu_item_hidden(
                    menu_title.as_ptr(),
                    item_title.as_ptr(),
                    i32::from(item.hidden),
                )
            };
            if configured == 0 {
                return Err(format!(
                    "failed to configure menu visibility: {} > {}",
                    item.menu_title, item.item_title
                ));
            }
        }
        Ok(())
    }

    pub fn plugin_developer_tool_available() -> bool {
        unsafe { iima_native_plugin_developer_tool_available() != 0 }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{
        NativeMenuItemState, NativeMenuKeyEquivalent, NativeMenuResponderAction,
        NativeMenuVisibility,
    };

    pub fn configure_key_equivalents(_items: &[NativeMenuKeyEquivalent]) -> Result<(), String> {
        Ok(())
    }

    pub fn configure_alternate_items(_items: &[(String, String, bool)]) -> Result<(), String> {
        Ok(())
    }

    pub fn configure_item_states(_items: &[NativeMenuItemState]) -> Result<(), String> {
        Ok(())
    }

    pub fn configure_responder_actions(_items: &[NativeMenuResponderAction]) -> Result<(), String> {
        Ok(())
    }

    pub fn configure_visibility(_items: &[NativeMenuVisibility]) -> Result<(), String> {
        Ok(())
    }

    pub fn plugin_developer_tool_available() -> bool {
        false
    }
}

pub use platform::{
    configure_alternate_items, configure_item_states, configure_key_equivalents,
    configure_responder_actions, configure_visibility, plugin_developer_tool_available,
};

#[cfg(test)]
mod tests {
    #[test]
    fn objective_c_bridge_targets_the_first_responder_and_hides_duplicate_container() {
        let source = include_str!("native_menu.m");
        for contract in [
            "iima_native_set_menu_item_responder_action",
            "NSSelectorFromString(selectorName)",
            "item.target = nil",
            "item.action = selector",
            "item.enabled = YES",
            "item.keyEquivalentModifierMask = 0",
            "iima_native_set_menu_item_hidden",
            "item.hidden = hidden",
        ] {
            assert!(
                source.contains(contract),
                "missing native menu contract: {contract}"
            );
        }
    }
}
