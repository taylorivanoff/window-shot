use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::RgbaImage;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, State,
    WebviewWindow,
};
use tauri_plugin_dialog::DialogExt;

use crate::render::{self, Style};
use crate::windows::{self, CaptureSource};

pub struct CaptureState {
    last: Mutex<Option<CapturedImage>>,
    source_image: Mutex<Option<SourceImage>>,
    style: Mutex<Option<StyleDto>>,
    source: Mutex<String>,
}

impl Default for CaptureState {
    fn default() -> Self {
        Self {
            last: Mutex::new(None),
            source_image: Mutex::new(None),
            style: Mutex::new(None),
            source: Mutex::new("auto".to_string()),
        }
    }
}

#[derive(Clone)]
struct CapturedImage {
    bytes: Vec<u8>,
    suggested_name: String,
}

/// The unstyled image kept so style changes can re-render the preview live.
#[derive(Clone)]
struct SourceImage {
    image: RgbaImage,
    scale: f32,
    name: String,
}

struct CaptureBundle {
    result: CaptureResult,
    bytes: Vec<u8>,
    source_image: SourceImage,
}

#[derive(Debug, Serialize)]
pub struct WindowDto {
    pub hwnd: String,
    pub title: String,
    pub process: String,
    pub pid: u32,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StyleDto {
    pub radius: f32,
    pub padding: f32,
    pub shadow_blur: f32,
    pub shadow_offset: f32,
    pub shadow_opacity: f32,
    pub shadow_color: [u8; 3],
    #[serde(default)]
    pub background_color: Option<[u8; 3]>,
}

impl Default for StyleDto {
    fn default() -> Self {
        Self {
            radius: 12.0,
            padding: 28.0,
            shadow_blur: 18.0,
            shadow_offset: 6.0,
            shadow_opacity: 0.35,
            shadow_color: [0, 0, 0],
            background_color: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CaptureResult {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub suggested_name: String,
    pub file_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RectDto {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub fn list_windows() -> Result<Vec<WindowDto>, String> {
    let current_pid = windows::current_pid();
    let windows = windows::list_windows_in_z_order().map_err(|error| error.to_string())?;
    Ok(windows
        .into_iter()
        .filter(|window| window.pid != current_pid)
        .map(|window| WindowDto {
            hwnd: format!("0x{:X}", window.hwnd_value),
            title: window.title,
            process: window.process,
            pid: window.pid,
            left: window.frame.left,
            top: window.frame.top,
            width: window.frame.width(),
            height: window.frame.height(),
            dpi: window.dpi,
        })
        .collect())
}

#[tauri::command]
pub fn overlay_bounds() -> RectDto {
    let bounds = windows::virtual_screen_bounds();
    RectDto {
        left: bounds.left,
        top: bounds.top,
        width: bounds.width(),
        height: bounds.height(),
    }
}

#[tauri::command]
pub fn capture_window(
    hwnd: String,
    style: StyleDto,
    source: String,
    state: State<'_, CaptureState>,
) -> Result<CaptureResult, String> {
    let hwnd_value = parse_hwnd(&hwnd)?;
    let window = windows::list_windows()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|window| window.hwnd_value == hwnd_value)
        .ok_or_else(|| format!("window {hwnd} is no longer available"))?;

    let CaptureBundle {
        result,
        bytes,
        source_image,
    } = capture_to_result(&window, style.clone(), source.clone())?;
    *state.last.lock() = Some(CapturedImage {
        bytes,
        suggested_name: result.suggested_name.clone(),
    });
    *state.source_image.lock() = Some(source_image);
    *state.style.lock() = Some(style);
    *state.source.lock() = source;
    Ok(result)
}

#[tauri::command]
pub fn import_image(
    data_url: String,
    style: StyleDto,
    suggested_name: Option<String>,
    state: State<'_, CaptureState>,
) -> Result<CaptureResult, String> {
    let style_value = style.clone().into_style()?;
    let image = decode_image_data_url(&data_url)?;
    let source = SourceImage {
        image,
        scale: 1.0,
        name: import_filename(suggested_name.as_deref()),
    };
    let (result, bytes) = render_source(&source, &style_value)?;

    *state.last.lock() = Some(CapturedImage {
        bytes,
        suggested_name: result.suggested_name.clone(),
    });
    *state.source_image.lock() = Some(source);
    *state.style.lock() = Some(style);
    Ok(result)
}

#[tauri::command]
pub fn restyle_image(
    style: StyleDto,
    state: State<'_, CaptureState>,
) -> Result<CaptureResult, String> {
    let style_value = style.clone().into_style()?;
    let source = state
        .source_image
        .lock()
        .clone()
        .ok_or_else(|| "import or capture an image before styling".to_string())?;
    let (result, bytes) = render_source(&source, &style_value)?;

    *state.last.lock() = Some(CapturedImage {
        bytes,
        suggested_name: result.suggested_name.clone(),
    });
    *state.style.lock() = Some(style);
    Ok(result)
}

#[tauri::command]
pub fn start_picker(style: StyleDto, source: String, app: AppHandle) -> Result<(), String> {
    style.clone().into_style()?;
    parse_source(&source)?;
    start_picker_impl(&app, Some(style), Some(source));
    Ok(())
}

/// Used by the global shortcut, where the last-used style/source are reused.
pub fn start_picker_impl(app: &AppHandle, style: Option<StyleDto>, source: Option<String>) {
    let state = app.state::<CaptureState>();
    if let Some(style) = style {
        *state.style.lock() = Some(style);
    }
    if let Some(source) = source {
        *state.source.lock() = source;
    }

    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }

    if let Some(overlay) = app.get_webview_window("overlay") {
        let bounds = windows::virtual_screen_bounds();
        let _ = overlay.set_position(Position::Physical(PhysicalPosition::new(
            bounds.left,
            bounds.top,
        )));
        let _ = overlay.set_size(Size::Physical(PhysicalSize::new(
            bounds.width(),
            bounds.height(),
        )));
        let _ = overlay.show();
        let _ = overlay.set_focus();
        let _ = app.emit_to("overlay", "picker:start", ());
    }
}

#[tauri::command]
pub fn pick_window_at(
    hwnd: String,
    app: AppHandle,
    state: State<'_, CaptureState>,
) -> Result<(), String> {
    let hwnd_value = parse_hwnd(&hwnd)?;
    let window = match windows::list_windows_in_z_order()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|window| window.hwnd_value == hwnd_value)
    {
        Some(window) => window,
        None => {
            return Err(fail_picker(
                &app,
                format!("window {hwnd} is no longer available"),
            ))
        }
    };

    // The screen fallback must not capture the picker overlay itself.
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }

    let style = state.style.lock().clone().unwrap_or_default();
    let source = state.source.lock().clone();
    let CaptureBundle {
        mut result,
        bytes,
        source_image,
    } = match capture_to_result(&window, style.clone(), source) {
        Ok(capture) => capture,
        Err(error) => return Err(fail_picker(&app, error)),
    };

    let path = match save_to_pictures(&result.suggested_name, &bytes) {
        Ok(path) => path,
        Err(error) => return Err(fail_picker(&app, error)),
    };
    result.file_path = Some(path.to_string_lossy().into_owned());
    open_in_photos(&path);

    *state.last.lock() = Some(CapturedImage {
        bytes,
        suggested_name: result.suggested_name.clone(),
    });
    *state.source_image.lock() = Some(source_image);
    *state.style.lock() = Some(style);
    let _ = app.emit_to("main", "picker:result", &result);
    finish_picker_impl(&app);
    Ok(())
}

#[tauri::command]
pub fn cancel_picker(app: AppHandle) {
    finish_picker_impl(&app);
}

fn finish_picker_impl(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
}

fn fail_picker(app: &AppHandle, error: String) -> String {
    let _ = app.emit_to("main", "toast", json!({ "message": error, "error": true }));
    finish_picker_impl(app);
    error
}

fn capture_to_result(
    window: &windows::WindowInfo,
    style: StyleDto,
    source: String,
) -> Result<CaptureBundle, String> {
    let capture_source = parse_source(&source)?;
    let style = style.into_style()?;
    let captured =
        windows::capture_window(window, capture_source).map_err(|error| error.to_string())?;
    let source_image = SourceImage {
        image: captured,
        scale: window.scale(),
        name: default_filename(window),
    };
    let (result, bytes) = render_source(&source_image, &style)?;

    Ok(CaptureBundle {
        result,
        bytes,
        source_image,
    })
}

fn render_source(source: &SourceImage, style: &Style) -> Result<(CaptureResult, Vec<u8>), String> {
    let pixels = source.image.width() as u64 * source.image.height() as u64;
    if pixels > 40_000_000 {
        return Err("image is too large to style (maximum 40 megapixels)".to_string());
    }

    let styled = render::apply_style(&source.image, style, source.scale);
    let bytes = encode_png(&styled)?;
    let result = CaptureResult {
        data_url: format!("data:image/png;base64,{}", STANDARD.encode(&bytes)),
        width: styled.width(),
        height: styled.height(),
        suggested_name: source.name.clone(),
        file_path: None,
    };
    Ok((result, bytes))
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut encoded = Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(encoded.into_inner())
}

fn decode_image_data_url(value: &str) -> Result<RgbaImage, String> {
    let trimmed = value.trim();
    let payload = if trimmed.starts_with("data:") {
        let (header, payload) = trimmed
            .split_once(',')
            .ok_or_else(|| "invalid image data URL".to_string())?;
        if !header.contains(";base64") {
            return Err("only base64 image data URLs are supported".to_string());
        }
        payload
    } else {
        trimmed
    };

    let bytes = STANDARD
        .decode(payload.trim())
        .map_err(|error| format!("invalid image data: {error}"))?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|error| format!("unsupported image format: {error}"))?;
    Ok(decoded.to_rgba8())
}

fn import_filename(name: Option<&str>) -> String {
    let stem = name
        .and_then(|value| Path::new(value).file_stem())
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "screenshot".to_string());
    format!("{}-{}.png", sanitize(&stem), timestamp())
}

#[tauri::command]
pub fn save_last(
    window: WebviewWindow,
    state: State<'_, CaptureState>,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let captured = state
        .last
        .lock()
        .clone()
        .ok_or_else(|| "capture or import an image before saving".to_string())?;
    let name = suggested_name.unwrap_or(captured.suggested_name);

    let picked = window
        .dialog()
        .file()
        .set_file_name(&name)
        .add_filter("PNG image", &["png"])
        .blocking_save_file();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|error| format!("unsupported save location: {error}"))?;
    std::fs::write(&path, &captured.bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

fn save_to_pictures(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .map_err(|_| "USERPROFILE is not set; cannot locate Pictures".to_string())?;
    let directory = Path::new(&home).join("Pictures").join("WindowShot");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
    let path = directory.join(name);
    std::fs::write(&path, bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    Ok(path)
}

fn open_in_photos(path: &Path) {
    // Ask Photos directly first; fall back to the registered image handler.
    let file_name = path.to_string_lossy();
    let uri = format!(
        "ms-photos:viewer?fileName={}",
        encode_uri_component(&file_name)
    );
    if Command::new("explorer.exe").arg(&uri).spawn().is_err() {
        let _ = Command::new("explorer.exe").arg(path).spawn();
    }
}

fn encode_uri_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b':'
            | b'\\'
            | b'/' => encoded.push(*byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

impl StyleDto {
    fn into_style(self) -> Result<Style, String> {
        ensure_non_negative(self.radius, "radius")?;
        ensure_non_negative(self.padding, "padding")?;
        ensure_non_negative(self.shadow_blur, "shadow blur")?;
        if !(0.0..=1.0).contains(&self.shadow_opacity) {
            return Err("shadow opacity must be between 0 and 1".to_string());
        }
        Ok(Style {
            radius: self.radius,
            padding: self.padding,
            shadow_blur: self.shadow_blur,
            shadow_offset: self.shadow_offset,
            shadow_opacity: self.shadow_opacity,
            shadow_color: self.shadow_color,
            background_color: self.background_color,
        })
    }
}

fn parse_source(source: &str) -> Result<CaptureSource, String> {
    source
        .parse()
        .map_err(|error: anyhow::Error| error.to_string())
}

fn ensure_non_negative(value: f32, name: &str) -> Result<(), String> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be a non-negative number"))
    }
}

fn parse_hwnd(value: &str) -> Result<usize, String> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return usize::from_str_radix(hex, 16)
            .map_err(|error| format!("invalid window handle '{value}': {error}"));
    }
    trimmed
        .parse::<usize>()
        .map_err(|error| format!("invalid window handle '{value}': {error}"))
}

fn default_filename(window: &windows::WindowInfo) -> String {
    format!(
        "window-{hwnd:X}-{title}-{timestamp}.png",
        hwnd = window.hwnd_value,
        title = sanitize(&window.title),
        timestamp = timestamp()
    )
}

fn sanitize(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let short: String = cleaned.trim_matches('_').chars().take(40).collect();
    if short.is_empty() {
        "window".to_string()
    } else {
        short
    }
}

fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn style_json(background_color: serde_json::Value) -> serde_json::Value {
        json!({
            "radius": 12,
            "padding": 28,
            "shadow_blur": 18,
            "shadow_offset": 6,
            "shadow_opacity": 0.35,
            "shadow_color": [0, 0, 0],
            "background_color": background_color,
        })
    }

    #[test]
    fn style_dto_accepts_transparent_background() {
        let style: StyleDto = serde_json::from_value(style_json(json!(null))).unwrap();
        assert_eq!(style.background_color, None);
    }

    #[test]
    fn style_dto_accepts_solid_background() {
        let style: StyleDto = serde_json::from_value(style_json(json!([250, 250, 255]))).unwrap();
        assert_eq!(style.background_color, Some([250, 250, 255]));
    }

    #[test]
    fn decode_image_data_url_reads_png() {
        let image = RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let data_url = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(encoded.into_inner())
        );

        let decoded = decode_image_data_url(&data_url).unwrap();
        assert_eq!(decoded.dimensions(), (2, 2));
    }
}
