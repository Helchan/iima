use serde::Serialize;
#[cfg(not(target_os = "macos"))]
use tauri::{LogicalPosition, LogicalSize};
use tauri::{Runtime, WebviewWindow};

const IINA_MINIMUM_WINDOW_WIDTH: f64 = 285.0;
const IINA_MINIMUM_WINDOW_HEIGHT: f64 = 120.0;
const IINA_WINDOW_SCALE_STEP: f64 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSizeAction {
    Half,
    Normal,
    Double,
    FitToScreen,
    Bigger,
    Smaller,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackWindowResizeAction {
    Preference(i64),
    PreserveWidth,
    VideoReconfigured,
}

impl PlaybackWindowResizeAction {
    fn raw_value(self) -> &'static str {
        match self {
            Self::Preference(_) => "preference",
            Self::PreserveWidth => "preserve-width",
            Self::VideoReconfigured => "video-reconfigured",
        }
    }
}

impl WindowSizeAction {
    pub fn raw_value(self) -> &'static str {
        match self {
            Self::Half => "half",
            Self::Normal => "normal",
            Self::Double => "double",
            Self::FitToScreen => "fit-to-screen",
            Self::Bigger => "bigger",
            Self::Smaller => "smaller",
        }
    }

    fn scale(self) -> Option<f64> {
        match self {
            Self::Half => Some(0.5),
            Self::Normal => Some(1.0),
            Self::Double => Some(2.0),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WindowFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn current_player_window_frame<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<WindowFrame, String> {
    #[cfg(target_os = "macos")]
    {
        let native_window = window.ns_window().map_err(|error| error.to_string())?;
        return native::read_context(native_window, None, false).map(|context| context.frame);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let scale = window
            .scale_factor()
            .map_err(|error| error.to_string())?
            .max(f64::EPSILON);
        let position = window.outer_position().map_err(|error| error.to_string())?;
        let size = window.outer_size().map_err(|error| error.to_string())?;
        Ok(WindowFrame {
            x: f64::from(position.x) / scale,
            y: f64::from(position.y) / scale,
            width: f64::from(size.width) / scale,
            height: f64::from(size.height) / scale,
        })
    }
}

pub fn set_player_window_frame<R: Runtime>(
    window: &WebviewWindow<R>,
    frame: WindowFrame,
) -> Result<(), String> {
    if ![frame.x, frame.y, frame.width, frame.height]
        .into_iter()
        .all(f64::is_finite)
        || frame.width <= 0.0
        || frame.height <= 0.0
    {
        return Err("Player window frame is invalid".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        return native::set_frame(
            window.ns_window().map_err(|error| error.to_string())?,
            frame,
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        window
            .set_position(LogicalPosition::new(frame.x, frame.y))
            .map_err(|error| error.to_string())?;
        window
            .set_size(LogicalSize::new(frame.width, frame.height))
            .map_err(|error| error.to_string())
    }
}

impl WindowFrame {
    fn centered_resize(self, width: f64, height: f64) -> Self {
        Self {
            x: self.x + (self.width - width) / 2.0,
            y: self.y + (self.height - height) / 2.0,
            width,
            height,
        }
    }

    fn centered_in(self, container: Self) -> Self {
        Self {
            x: container.x + (container.width - self.width) / 2.0,
            y: container.y + (container.height - self.height) / 2.0,
            ..self
        }
    }

    fn constrained_to(mut self, container: Self) -> Self {
        if self.width > container.width || self.height > container.height {
            (self.width, self.height) =
                shrink_size(self.width, self.height, container.width, container.height);
        }
        if self.x < container.x {
            self.x = container.x;
        }
        if self.y < container.y {
            self.y = container.y;
        }
        if self.x + self.width > container.x + container.width {
            self.x = container.x + container.width - self.width;
        }
        if self.y + self.height > container.y + container.height {
            self.y = container.y + container.height - self.height;
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GeometryMagnitude {
    value: f64,
    percent: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct IinaWindowGeometry {
    width: Option<GeometryMagnitude>,
    height: Option<GeometryMagnitude>,
    x: Option<(char, GeometryMagnitude)>,
    y: Option<(char, GeometryMagnitude)>,
}

fn parse_geometry_magnitude(value: &str) -> Option<GeometryMagnitude> {
    let (digits, percent) = value
        .strip_suffix('%')
        .map_or((value, false), |digits| (digits, true));
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(GeometryMagnitude {
        value: digits.parse().ok()?,
        percent,
    })
}

fn parse_iina_geometry(value: &str) -> Option<IinaWindowGeometry> {
    if value.is_empty() {
        return Some(IinaWindowGeometry::default());
    }
    let position_start = value
        .char_indices()
        .find_map(|(index, character)| matches!(character, '+' | '-').then_some(index));
    let (size, position) = position_start.map_or((value, ""), |index| value.split_at(index));
    let (width, height) = if size.is_empty() {
        (None, None)
    } else if let Some(height) = size.strip_prefix('x') {
        (None, Some(parse_geometry_magnitude(height)?))
    } else if let Some((width, height)) = size.split_once('x') {
        (
            Some(parse_geometry_magnitude(width)?),
            Some(parse_geometry_magnitude(height)?),
        )
    } else {
        (Some(parse_geometry_magnitude(size)?), None)
    };
    let (x, y) = if position.is_empty() {
        (None, None)
    } else {
        let x_sign = position.chars().next()?;
        if !matches!(x_sign, '+' | '-') {
            return None;
        }
        let second_sign = position[1..]
            .char_indices()
            .find_map(|(index, character)| matches!(character, '+' | '-').then_some(index + 1))?;
        let y_sign = position[second_sign..].chars().next()?;
        (
            Some((x_sign, parse_geometry_magnitude(&position[1..second_sign])?)),
            Some((
                y_sign,
                parse_geometry_magnitude(&position[second_sign + 1..])?,
            )),
        )
    };
    if width.is_none() && height.is_none() && x.is_none() {
        return None;
    }
    Some(IinaWindowGeometry {
        width,
        height,
        x,
        y,
    })
}

pub fn valid_iina_geometry(value: &str) -> bool {
    parse_iina_geometry(value).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NativeWindowContext {
    frame: WindowFrame,
    visible_frame: WindowFrame,
    video_width: f64,
    video_height: f64,
    aspect_ratio: f64,
    fullscreen: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowResizeResult {
    pub action: String,
    pub changed: bool,
    pub frame: WindowFrame,
}

fn shrink_size(width: f64, height: f64, max_width: f64, max_height: f64) -> (f64, f64) {
    if width <= max_width && height <= max_height {
        return (width, height);
    }
    let factor = (max_width / width).min(max_height / height);
    (width * factor, height * factor)
}

fn satisfy_minimum_size(width: f64, height: f64) -> (f64, f64) {
    if width >= IINA_MINIMUM_WINDOW_WIDTH && height >= IINA_MINIMUM_WINDOW_HEIGHT {
        return (width, height);
    }
    let factor = (IINA_MINIMUM_WINDOW_WIDTH / width).max(IINA_MINIMUM_WINDOW_HEIGHT / height);
    (width * factor, height * factor)
}

fn target_frame(
    context: NativeWindowContext,
    action: WindowSizeAction,
) -> Result<Option<WindowFrame>, String> {
    if context.fullscreen {
        return Ok(None);
    }
    let current = context.frame;
    let screen = context.visible_frame;
    if ![
        current.width,
        current.height,
        screen.width,
        screen.height,
        context.aspect_ratio,
    ]
    .iter()
    .all(|value| value.is_finite() && *value > 0.0)
    {
        return Err("Native window geometry is unavailable".to_string());
    }

    if let Some(scale) = action.scale() {
        if !context.video_width.is_finite()
            || !context.video_height.is_finite()
            || context.video_width <= 0.0
            || context.video_height <= 0.0
        {
            return Err("Video dimensions are unavailable".to_string());
        }
        let requested_width = context.video_width * scale;
        let requested_height = context.video_height * scale;
        let (width, height) = if requested_width > screen.width || requested_height > screen.height
        {
            // This deliberately follows IINA 1.3.5: when the requested video scale is too large,
            // shrink the current window to the screen instead of using the oversized request.
            shrink_size(current.width, current.height, screen.width, screen.height)
        } else {
            satisfy_minimum_size(requested_width, requested_height)
        };
        return Ok(Some(
            current
                .centered_resize(width, height)
                .constrained_to(screen),
        ));
    }

    match action {
        WindowSizeAction::FitToScreen => {
            let (width, height) =
                shrink_size(current.width, current.height, screen.width, screen.height);
            Ok(Some(
                current.centered_resize(width, height).centered_in(screen),
            ))
        }
        WindowSizeAction::Bigger | WindowSizeAction::Smaller => {
            let direction = if action == WindowSizeAction::Bigger {
                1.0
            } else {
                -1.0
            };
            let width = current.width + IINA_WINDOW_SCALE_STEP * direction;
            let height = width / context.aspect_ratio;
            let (width, height) = satisfy_minimum_size(width, height);
            Ok(Some(current.centered_resize(width, height)))
        }
        WindowSizeAction::Half | WindowSizeAction::Normal | WindowSizeAction::Double => {
            unreachable!("scale actions were handled above")
        }
    }
}

fn geometry_value(
    magnitude: GeometryMagnitude,
    screen_dimension: f64,
    window_dimension: f64,
) -> f64 {
    if magnitude.percent {
        magnitude.value * 0.01 * screen_dimension - window_dimension / 2.0
    } else {
        magnitude.value
    }
}

fn apply_initial_geometry(
    mut frame: WindowFrame,
    screen: WindowFrame,
    raw_geometry: &str,
) -> Result<WindowFrame, String> {
    let geometry = parse_iina_geometry(raw_geometry)
        .ok_or_else(|| "Initial window geometry is invalid".to_string())?;
    let aspect = frame.width / frame.height;
    let mut size_was_set = false;
    if let Some(width) = geometry.width.filter(|width| width.value != 0.0) {
        let width = if width.percent {
            width.value * 0.01 * screen.width
        } else {
            width.value
        }
        .max(IINA_MINIMUM_WINDOW_WIDTH);
        frame.width = width;
        frame.height = width / aspect;
        size_was_set = true;
    } else if let Some(height) = geometry.height.filter(|height| height.value != 0.0) {
        let height = if height.percent {
            height.value * 0.01 * screen.height
        } else {
            height.value
        }
        .max(IINA_MINIMUM_WINDOW_HEIGHT);
        frame.height = height;
        frame.width = height * aspect;
        size_was_set = true;
    }
    if let Some((sign, x)) = geometry.x {
        let offset = geometry_value(x, screen.width, frame.width);
        frame.x = if sign == '+' {
            screen.x + offset
        } else {
            screen.x + screen.width - offset - frame.width
        };
    }
    if let Some((sign, y)) = geometry.y {
        let offset = geometry_value(y, screen.height, frame.height);
        frame.y = if sign == '+' {
            screen.y + offset
        } else {
            screen.y + screen.height - offset - frame.height
        };
    }
    if geometry.x.is_none() && geometry.y.is_none() && size_was_set {
        frame = frame.centered_in(screen);
    }
    Ok(frame.constrained_to(screen))
}

fn preference_target_frame(
    context: NativeWindowContext,
    resize_option: i64,
    geometry: Option<&str>,
) -> Result<Option<WindowFrame>, String> {
    if context.fullscreen {
        return Ok(None);
    }
    let mut frame = context.frame;
    if context.video_width > 0.0
        && context.video_height > 0.0
        && context.video_width.is_finite()
        && context.video_height.is_finite()
    {
        let ratio = match resize_option {
            0 => None,
            1 => Some(0.5),
            3 => Some(1.5),
            4 => Some(2.0),
            _ => Some(1.0),
        };
        let (requested_width, requested_height) = ratio
            .map_or((context.video_width, context.video_height), |ratio| {
                (context.video_width * ratio, context.video_height * ratio)
            });
        let (width, height) = shrink_size(
            requested_width,
            requested_height,
            context.visible_frame.width,
            context.visible_frame.height,
        );
        let (width, height) = satisfy_minimum_size(width, height);
        frame = context.frame.centered_resize(width, height);
        if ratio.is_none() {
            frame = frame.centered_in(context.visible_frame);
        }
        frame = frame.constrained_to(context.visible_frame);
    }
    if let Some(geometry) = geometry.filter(|geometry| !geometry.is_empty()) {
        frame = apply_initial_geometry(frame, context.visible_frame, geometry)?;
    }
    Ok(Some(frame))
}

fn playback_target_frame(
    context: NativeWindowContext,
    action: PlaybackWindowResizeAction,
) -> Result<Option<WindowFrame>, String> {
    if context.fullscreen {
        return Ok(None);
    }
    if !context.video_width.is_finite()
        || !context.video_height.is_finite()
        || context.video_width <= 0.0
        || context.video_height <= 0.0
    {
        return Err("Video dimensions are unavailable".to_string());
    }
    if let PlaybackWindowResizeAction::Preference(resize_option) = action {
        return preference_target_frame(context, resize_option, None);
    }

    let frame = match action {
        PlaybackWindowResizeAction::PreserveWidth => {
            let height = context.frame.width / context.video_width * context.video_height;
            let (width, height) = satisfy_minimum_size(context.frame.width, height);
            WindowFrame {
                width,
                height,
                ..context.frame
            }
            .constrained_to(context.visible_frame)
        }
        PlaybackWindowResizeAction::VideoReconfigured => {
            let (width, height) = shrink_size(
                context.video_width,
                context.video_height,
                context.visible_frame.width,
                context.visible_frame.height,
            );
            let (width, height) = satisfy_minimum_size(width, height);
            context
                .frame
                .centered_resize(width, height)
                .constrained_to(context.visible_frame)
        }
        PlaybackWindowResizeAction::Preference(_) => {
            unreachable!("preference resizing was handled above")
        }
    };
    Ok(Some(frame))
}

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::c_void;

    use super::{NativeWindowContext, WindowFrame};

    unsafe extern "C" {
        fn iima_native_read_player_window_context(
            window: *mut c_void,
            video_width: f64,
            video_height: f64,
            use_physical_resolution: i32,
            values: *mut f64,
        ) -> i32;
        fn iima_native_set_player_window_frame(
            window: *mut c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        ) -> i32;
        fn iima_native_set_player_window_frame_immediate(
            window: *mut c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        ) -> i32;
    }

    pub fn read_context(
        window: *mut c_void,
        video_size: Option<(f64, f64)>,
        use_physical_resolution: bool,
    ) -> Result<NativeWindowContext, String> {
        if window.is_null() {
            return Err("Native window pointer is null".to_string());
        }
        let (video_width, video_height) = video_size.unwrap_or_default();
        // frame(4), visible frame(4), logical video size(2), aspect ratio(1), fullscreen(1)
        let mut values = [0.0_f64; 12];
        let status = unsafe {
            iima_native_read_player_window_context(
                window,
                video_width,
                video_height,
                i32::from(use_physical_resolution),
                values.as_mut_ptr(),
            )
        };
        if status != 0 {
            return Err(format!("Unable to read native window geometry ({status})"));
        }
        Ok(NativeWindowContext {
            frame: WindowFrame {
                x: values[0],
                y: values[1],
                width: values[2],
                height: values[3],
            },
            visible_frame: WindowFrame {
                x: values[4],
                y: values[5],
                width: values[6],
                height: values[7],
            },
            video_width: values[8],
            video_height: values[9],
            aspect_ratio: values[10],
            fullscreen: values[11] != 0.0,
        })
    }

    pub fn set_frame(window: *mut c_void, frame: WindowFrame) -> Result<(), String> {
        let status = unsafe {
            iima_native_set_player_window_frame(window, frame.x, frame.y, frame.width, frame.height)
        };
        (status == 0)
            .then_some(())
            .ok_or_else(|| format!("Unable to resize native window ({status})"))
    }

    pub fn set_frame_immediate(window: *mut c_void, frame: WindowFrame) -> Result<(), String> {
        let status = unsafe {
            iima_native_set_player_window_frame_immediate(
                window,
                frame.x,
                frame.y,
                frame.width,
                frame.height,
            )
        };
        (status == 0)
            .then_some(())
            .ok_or_else(|| format!("Unable to resize native window immediately ({status})"))
    }
}

fn magnification_target_frame(
    context: NativeWindowContext,
    magnification: f64,
) -> Result<Option<WindowFrame>, String> {
    if context.fullscreen || !magnification.is_finite() {
        return Ok(None);
    }
    let factor = 1.0 + magnification;
    if factor <= 0.0 || !context.aspect_ratio.is_finite() || context.aspect_ratio <= 0.0 {
        return Ok(None);
    }
    let width = context.frame.width * factor;
    let height = width / context.aspect_ratio;
    if width <= IINA_MINIMUM_WINDOW_WIDTH
        || height <= IINA_MINIMUM_WINDOW_HEIGHT
        || height >= context.visible_frame.height
    {
        return Ok(None);
    }
    Ok(Some(WindowFrame {
        x: context.frame.x + (context.frame.width - width) / 2.0,
        y: context.frame.y + (context.frame.height - height) / 2.0,
        width,
        height,
    }))
}

pub fn resize_player_window_by_magnification<R: Runtime>(
    window: &WebviewWindow<R>,
    magnification: f64,
) -> Result<WindowResizeResult, String> {
    #[cfg(target_os = "macos")]
    {
        let native_window = window.ns_window().map_err(|error| error.to_string())?;
        let context = native::read_context(native_window, None, false)?;
        let Some(frame) = magnification_target_frame(context, magnification)? else {
            return Ok(WindowResizeResult {
                action: "magnify".to_string(),
                changed: false,
                frame: context.frame,
            });
        };
        native::set_frame_immediate(native_window, frame)?;
        Ok(WindowResizeResult {
            action: "magnify".to_string(),
            changed: frame != context.frame,
            frame,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, magnification);
        Err("IINA window magnification is only available on macOS".to_string())
    }
}

pub fn resize_player_window<R: Runtime>(
    window: &WebviewWindow<R>,
    action: WindowSizeAction,
    video_size: Option<(f64, f64)>,
    use_physical_resolution: bool,
) -> Result<WindowResizeResult, String> {
    #[cfg(target_os = "macos")]
    {
        let native_window = window.ns_window().map_err(|error| error.to_string())?;
        let context = native::read_context(native_window, video_size, use_physical_resolution)?;
        let Some(frame) = target_frame(context, action)? else {
            return Ok(WindowResizeResult {
                action: action.raw_value().to_string(),
                changed: false,
                frame: context.frame,
            });
        };
        native::set_frame(native_window, frame)?;
        Ok(WindowResizeResult {
            action: action.raw_value().to_string(),
            changed: frame != context.frame,
            frame,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, action, video_size, use_physical_resolution);
        Err("IINA window sizing is only available on macOS".to_string())
    }
}

pub fn resize_player_window_for_open<R: Runtime>(
    window: &WebviewWindow<R>,
    video_size: Option<(f64, f64)>,
    use_physical_resolution: bool,
    resize_option: i64,
    geometry: Option<&str>,
) -> Result<WindowResizeResult, String> {
    #[cfg(target_os = "macos")]
    {
        let native_window = window.ns_window().map_err(|error| error.to_string())?;
        let context = native::read_context(native_window, video_size, use_physical_resolution)?;
        let Some(frame) = preference_target_frame(context, resize_option, geometry)? else {
            return Ok(WindowResizeResult {
                action: "preference".to_string(),
                changed: false,
                frame: context.frame,
            });
        };
        native::set_frame(native_window, frame)?;
        Ok(WindowResizeResult {
            action: "preference".to_string(),
            changed: frame != context.frame,
            frame,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (
            window,
            video_size,
            use_physical_resolution,
            resize_option,
            geometry,
        );
        Err("IINA window sizing is only available on macOS".to_string())
    }
}

pub fn resize_player_window_for_playback<R: Runtime>(
    window: &WebviewWindow<R>,
    video_size: (f64, f64),
    use_physical_resolution: bool,
    action: PlaybackWindowResizeAction,
) -> Result<WindowResizeResult, String> {
    #[cfg(target_os = "macos")]
    {
        let native_window = window.ns_window().map_err(|error| error.to_string())?;
        let context =
            native::read_context(native_window, Some(video_size), use_physical_resolution)?;
        let Some(frame) = playback_target_frame(context, action)? else {
            return Ok(WindowResizeResult {
                action: action.raw_value().to_string(),
                changed: false,
                frame: context.frame,
            });
        };
        native::set_frame(native_window, frame)?;
        Ok(WindowResizeResult {
            action: action.raw_value().to_string(),
            changed: frame != context.frame,
            frame,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, video_size, use_physical_resolution, action);
        Err("IINA window sizing is only available on macOS".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> NativeWindowContext {
        NativeWindowContext {
            frame: WindowFrame {
                x: 100.0,
                y: 100.0,
                width: 900.0,
                height: 506.25,
            },
            visible_frame: WindowFrame {
                x: 0.0,
                y: 25.0,
                width: 1440.0,
                height: 875.0,
            },
            video_width: 960.0,
            video_height: 540.0,
            aspect_ratio: 16.0 / 9.0,
            fullscreen: false,
        }
    }

    #[test]
    fn video_scales_are_centered_and_use_logical_video_dimensions() {
        let half = target_frame(context(), WindowSizeAction::Half)
            .unwrap()
            .unwrap();
        assert_eq!((half.width, half.height), (480.0, 270.0));
        assert_eq!((half.x, half.y), (310.0, 218.125));

        let normal = target_frame(context(), WindowSizeAction::Normal)
            .unwrap()
            .unwrap();
        assert_eq!((normal.width, normal.height), (960.0, 540.0));
    }

    #[test]
    fn magnification_resizes_about_center_and_enforces_reference_thresholds() {
        let enlarged = magnification_target_frame(context(), 0.1).unwrap().unwrap();
        assert!((enlarged.width - 990.0).abs() < 0.000_001);
        assert!((enlarged.height - 556.875).abs() < 0.000_001);
        assert!((enlarged.x - 55.0).abs() < 0.000_001);
        assert!((enlarged.y - 74.6875).abs() < 0.000_001);

        let mut fullscreen = context();
        fullscreen.fullscreen = true;
        assert_eq!(magnification_target_frame(fullscreen, 0.1).unwrap(), None);
        assert_eq!(magnification_target_frame(context(), -0.95).unwrap(), None);
    }

    #[test]
    fn fit_only_shrinks_and_centers_the_current_window() {
        let mut small = context();
        small.frame = WindowFrame {
            x: 10.0,
            y: 40.0,
            width: 640.0,
            height: 360.0,
        };
        let result = target_frame(small, WindowSizeAction::FitToScreen)
            .unwrap()
            .unwrap();
        assert_eq!((result.width, result.height), (640.0, 360.0));
        assert_eq!((result.x, result.y), (400.0, 282.5));

        let mut large = context();
        large.frame.width = 1920.0;
        large.frame.height = 1080.0;
        let result = target_frame(large, WindowSizeAction::FitToScreen)
            .unwrap()
            .unwrap();
        assert_eq!((result.width, result.height), (1440.0, 810.0));
    }

    #[test]
    fn step_sizes_preserve_aspect_and_minimum_size() {
        let bigger = target_frame(context(), WindowSizeAction::Bigger)
            .unwrap()
            .unwrap();
        assert_eq!((bigger.width, bigger.height), (925.0, 520.3125));

        let mut minimum = context();
        minimum.frame.width = 285.0;
        minimum.frame.height = 160.3125;
        let smaller = target_frame(minimum, WindowSizeAction::Smaller)
            .unwrap()
            .unwrap();
        assert_eq!(smaller.width, 285.0);
        assert!((smaller.height - 160.3125).abs() < 0.000001);
    }

    #[test]
    fn fullscreen_is_an_exact_no_op() {
        let mut fullscreen = context();
        fullscreen.fullscreen = true;
        assert_eq!(
            target_frame(fullscreen, WindowSizeAction::Double).unwrap(),
            None
        );
    }

    #[test]
    fn iina_geometry_parser_accepts_the_reference_subset_only() {
        for geometry in [
            "",
            "1280",
            "x720",
            "80%",
            "x50%",
            "+20-30",
            "1280+20-30%",
            "1920x1080-10+20",
        ] {
            assert!(valid_iina_geometry(geometry), "{geometry}");
        }
        for geometry in ["x", "width", "+20", "1280+20", "1280++20", "20.5+0+0"] {
            assert!(!valid_iina_geometry(geometry), "{geometry}");
        }
    }

    #[test]
    fn open_preference_resizes_to_video_and_applies_initial_geometry_once() {
        let video_frame = preference_target_frame(context(), 2, None)
            .unwrap()
            .unwrap();
        assert_eq!((video_frame.width, video_frame.height), (960.0, 540.0));
        assert_eq!((video_frame.x, video_frame.y), (70.0, 83.125));

        let geometry_frame = preference_target_frame(context(), 2, Some("x50%+20-30"))
            .unwrap()
            .unwrap();
        assert!((geometry_frame.width - 777.777_777_777_777_8).abs() < 0.000_001);
        assert_eq!(geometry_frame.height, 437.5);
        assert_eq!(geometry_frame.x, 20.0);
        assert_eq!(geometry_frame.y, 432.5);
    }

    #[test]
    fn open_resize_options_match_iina_scale_tags_and_screen_fit() {
        for (option, expected_width, expected_height) in [
            (1, 480.0, 270.0),
            (2, 960.0, 540.0),
            (3, 1_440.0, 810.0),
            (4, 1_440.0, 810.0),
        ] {
            let frame = preference_target_frame(context(), option, None)
                .unwrap()
                .unwrap();
            assert_eq!(
                (frame.width, frame.height),
                (expected_width, expected_height)
            );
        }
        let fit = preference_target_frame(context(), 0, None)
            .unwrap()
            .unwrap();
        assert_eq!((fit.width, fit.height), (960.0, 540.0));
        assert_eq!((fit.x, fit.y), (240.0, 192.5));
    }

    #[test]
    fn playlist_transition_preserves_width_and_origin_while_matching_new_aspect() {
        let mut next_video = context();
        next_video.video_width = 1920.0;
        next_video.video_height = 800.0;
        let frame = playback_target_frame(next_video, PlaybackWindowResizeAction::PreserveWidth)
            .unwrap()
            .unwrap();
        assert_eq!((frame.x, frame.y), (100.0, 100.0));
        assert_eq!((frame.width, frame.height), (900.0, 375.0));
    }

    #[test]
    fn video_reconfiguration_uses_unscaled_video_size_and_recenters() {
        let mut reconfigured = context();
        reconfigured.video_width = 1280.0;
        reconfigured.video_height = 720.0;
        let frame =
            playback_target_frame(reconfigured, PlaybackWindowResizeAction::VideoReconfigured)
                .unwrap()
                .unwrap();
        assert_eq!((frame.width, frame.height), (1280.0, 720.0));
        assert_eq!((frame.x, frame.y), (0.0, 25.0));
    }

    #[test]
    fn automatic_always_uses_the_configured_file_start_ratio() {
        let frame = playback_target_frame(context(), PlaybackWindowResizeAction::Preference(1))
            .unwrap()
            .unwrap();
        assert_eq!((frame.width, frame.height), (480.0, 270.0));
        assert_eq!((frame.x, frame.y), (310.0, 218.125));
    }
}
