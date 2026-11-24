use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::render::{self, Style};
use crate::windows::{self, CaptureSource};

#[derive(Default)]
pub struct CaptureState {
    last: Mutex<Option<CapturedImage>>,
}

#[derive(Clone)]
struct CapturedImage {
    bytes: Vec<u8>,
    suggested_name: String,
}

#[derive(Debug, Serialize)]
pub struct WindowDto {
    pub hwnd: String,
    pub title: String,
    pub process: String,
    pub pid: u32,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
}

#[derive(Debug, Deserialize)]
pub struct StyleDto {
    pub radius: f32,
    pub padding: f32,
    pub shadow_blur: f32,
    pub shadow_offset: f32,
    pub shadow_opacity: f32,
    pub shadow_color: [u8; 3],
}

#[derive(Debug, Serialize)]
pub struct CaptureResult {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub suggested_name: String,
}

#[tauri::command]
pub fn list_windows() -> Result<Vec<WindowDto>, String> {
    let current_pid = windows::current_pid();
    let windows = windows::list_windows().map_err(|error| error.to_string())?;
    Ok(windows
        .into_iter()
        .filter(|window| window.pid != current_pid)
        .map(|window| WindowDto {
            hwnd: format!("0x{:X}", window.hwnd_value),
            title: window.title,
            process: window.process,
            pid: window.pid,
            width: window.frame.width(),
            height: window.frame.height(),
            dpi: window.dpi,
        })
        .collect())
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

    let source: CaptureSource = source
        .parse()
        .map_err(|error: anyhow::Error| error.to_string())?;
    let style = style.into_style()?;

    let captured = windows::capture_window(&window, source).map_err(|error| error.to_string())?;
    let styled = render::apply_style(&captured, &style, window.scale());
    let suggested_name = default_filename(&window);

    let mut encoded = Cursor::new(Vec::new());
    styled
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    let bytes = encoded.into_inner();
    let data_url = format!("data:image/png;base64,{}", STANDARD.encode(&bytes));

    *state.last.lock() = Some(CapturedImage {
        bytes: bytes.clone(),
        suggested_name: suggested_name.clone(),
    });

    Ok(CaptureResult {
        data_url,
        width: styled.width(),
        height: styled.height(),
        suggested_name,
    })
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
        .ok_or_else(|| "capture a window before saving".to_string())?;
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
        })
    }
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
