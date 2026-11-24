use std::ffi::c_void;
use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, bail, Context, Result};
use image::{Rgba, RgbaImage};
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowDisplayAffinity, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetProcessDPIAware,
    GWL_STYLE, PW_RENDERFULLCONTENT, WDA_EXCLUDEFROMCAPTURE, WS_MINIMIZE,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    fn from_win(rect: RECT) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }

    fn to_win(self) -> RECT {
        RECT {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }

    pub fn width(self) -> u32 {
        (self.right - self.left).max(0) as u32
    }

    pub fn height(self) -> u32 {
        (self.bottom - self.top).max(0) as u32
    }

    fn area(self) -> u64 {
        self.width() as u64 * self.height() as u64
    }
}

#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub hwnd: HWND,
    pub hwnd_value: usize,
    pub title: String,
    pub process: String,
    pub pid: u32,
    /// DWM's visible bounds (excludes the invisible resize border).
    pub frame: Rect,
    /// Raw Win32 window bounds, used as the capture source rectangle.
    pub raw: Rect,
    pub dpi: u32,
}

impl WindowInfo {
    /// Scale logical style values (radius/shadow/padding) to physical pixels.
    pub fn scale(&self) -> f32 {
        self.dpi.max(96) as f32 / 96.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureSource {
    /// Try PrintWindow, then fall back to a screen-region grab.
    Auto,
    /// PrintWindow only. Usually best for occluded or background windows.
    Print,
    /// Screen region only. Accurate colors, but the window must be unobscured.
    Screen,
}

impl FromStr for CaptureSource {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "print" | "printwindow" => Ok(Self::Print),
            "screen" | "bitblt" => Ok(Self::Screen),
            other => bail!("unknown capture source '{other}' (expected auto, print, or screen)"),
        }
    }
}

pub fn current_pid() -> u32 {
    unsafe { GetCurrentProcessId() }
}

/// Must be called before any window geometry or DPI queries.
pub fn enable_dpi_awareness() {
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            let _ = SetProcessDPIAware();
        }
    }
}

pub fn list_windows() -> Result<Vec<WindowInfo>> {
    let mut windows: Vec<WindowInfo> = Vec::new();
    unsafe {
        EnumWindows(
            Some(enum_window),
            LPARAM(&mut windows as *mut Vec<WindowInfo> as isize),
        )?;
    }
    windows.sort_by(|a, b| b.raw.area().cmp(&a.raw.area()));
    Ok(windows)
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<WindowInfo>);
    if let Some(info) = window_info(hwnd) {
        windows.push(info);
    }
    BOOL(1)
}

unsafe fn window_info(hwnd: HWND) -> Option<WindowInfo> {
    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return None;
    }

    let length = GetWindowTextLengthW(hwnd);
    if length <= 0 {
        return None;
    }
    let mut text = vec![0u16; length as usize + 1];
    let written = GetWindowTextW(hwnd, &mut text);
    if written <= 0 {
        return None;
    }
    let title = String::from_utf16_lossy(&text[..written as usize]);
    if title.trim().is_empty() {
        return None;
    }

    let mut cloaked = 0u32;
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut c_void,
        std::mem::size_of::<u32>() as u32,
    )
    .is_ok()
        && cloaked != 0
    {
        return None;
    }

    let mut affinity = 0u32;
    if GetWindowDisplayAffinity(hwnd, &mut affinity).is_ok() && affinity == WDA_EXCLUDEFROMCAPTURE.0
    {
        return None;
    }

    let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
    if style & WS_MINIMIZE.0 != 0 {
        return None;
    }

    let mut raw_win = RECT::default();
    GetWindowRect(hwnd, &mut raw_win).ok()?;
    let raw = Rect::from_win(raw_win);
    if raw.width() < 80 || raw.height() < 60 {
        return None;
    }

    let mut frame_win = raw.to_win();
    let frame = if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut frame_win as *mut RECT as *mut c_void,
        std::mem::size_of::<RECT>() as u32,
    )
    .is_ok()
    {
        let candidate = Rect::from_win(frame_win);
        if candidate.width() > 0 && candidate.height() > 0 {
            candidate
        } else {
            raw
        }
    } else {
        raw
    };

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));

    Some(WindowInfo {
        hwnd,
        hwnd_value: hwnd.0 as usize,
        title,
        process: process_name(pid),
        pid,
        frame,
        raw,
        dpi: GetDpiForWindow(hwnd).max(96),
    })
}

unsafe fn process_name(pid: u32) -> String {
    if pid == 0 {
        return "?".to_string();
    }
    let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
        return "?".to_string();
    };

    let mut buffer = vec![0u16; 260];
    let mut size = buffer.len() as u32;
    let name = if QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_FORMAT(0),
        PWSTR::from_raw(buffer.as_mut_ptr()),
        &mut size,
    )
    .is_ok()
        && size > 0
    {
        let chars = (size as usize).min(buffer.len());
        let raw = String::from_utf16_lossy(&buffer[..chars]);
        let full = raw.trim_end_matches('\0');
        Path::new(full)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| full.to_string())
    } else {
        "?".to_string()
    };

    let _ = CloseHandle(handle);
    name
}

/// Capture a window into a cropped, top-down RGBA buffer.
pub fn capture_window(window: &WindowInfo, source: CaptureSource) -> Result<RgbaImage> {
    match source {
        CaptureSource::Print => capture_bits(window, true),
        CaptureSource::Screen => capture_bits(window, false),
        CaptureSource::Auto => match capture_bits(window, true) {
            Ok(image) if !is_uniform(&image) => Ok(image),
            Ok(_) => capture_bits(window, false)
                .context("PrintWindow returned a blank frame and the screen fallback failed"),
            Err(print_error) => capture_bits(window, false).with_context(|| {
                format!("PrintWindow failed ({print_error}) and the screen fallback failed")
            }),
        },
    }
}

fn capture_bits(window: &WindowInfo, use_print_window: bool) -> Result<RgbaImage> {
    let width = window.raw.width();
    let height = window.raw.height();
    if width == 0 || height == 0 {
        bail!("window has an empty bounds rectangle");
    }

    unsafe {
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            bail!("could not acquire a screen device context");
        }
        let memory_dc = CreateCompatibleDC(Some(screen_dc));
        if memory_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            bail!("could not create a memory device context");
        }
        let bitmap = CreateCompatibleBitmap(screen_dc, width as i32, height as i32);
        if bitmap.is_invalid() {
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(None, screen_dc);
            bail!("could not create a compatible bitmap");
        }
        let old_bitmap = SelectObject(memory_dc, HGDIOBJ::from(bitmap));

        let acquired = if use_print_window {
            PrintWindow(
                window.hwnd,
                memory_dc,
                PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT),
            )
            .as_bool()
        } else {
            BitBlt(
                memory_dc,
                0,
                0,
                width as i32,
                height as i32,
                Some(screen_dc),
                window.raw.left,
                window.raw.top,
                SRCCOPY,
            )
            .is_ok()
        };

        let image = if acquired {
            let mut bits = vec![0u8; width as usize * height as usize * 4];
            let mut info = BITMAPINFO::default();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: bits.len() as u32,
                ..Default::default()
            };
            let lines = GetDIBits(
                memory_dc,
                bitmap,
                0,
                height,
                Some(bits.as_mut_ptr() as *mut c_void),
                &mut info,
                DIB_RGB_COLORS,
            );
            if lines == 0 {
                None
            } else {
                // GDI leaves the alpha channel zeroed for most windows.
                for pixel in bits.chunks_exact_mut(4) {
                    let blue = pixel[0];
                    let red = pixel[2];
                    pixel[0] = red;
                    pixel[2] = blue;
                    pixel[3] = 255;
                }
                Some(crop_capture(&bits, width as usize, window))
            }
        } else {
            None
        };

        SelectObject(memory_dc, old_bitmap);
        let _ = DeleteObject(HGDIOBJ::from(bitmap));
        let _ = DeleteDC(memory_dc);
        ReleaseDC(None, screen_dc);

        image.ok_or_else(|| {
            if use_print_window {
                anyhow!("PrintWindow failed for this window")
            } else {
                anyhow!("screen-region capture failed")
            }
        })
    }
}

/// Detect the common PrintWindow failure where the bitmap is a flat color.
fn is_uniform(image: &RgbaImage) -> bool {
    let (width, height) = (image.width() as usize, image.height() as usize);
    if width < 16 || height < 16 {
        return false;
    }
    let step_x = (width / 40).max(1);
    let step_y = (height / 40).max(1);
    let mut reference = [0u8; 3];
    let mut found = false;
    for y in (0..height).step_by(step_y) {
        for x in (0..width).step_by(step_x) {
            let pixel = image.get_pixel(x as u32, y as u32).0;
            if !found {
                reference = [pixel[0], pixel[1], pixel[2]];
                found = true;
            } else if reference
                .iter()
                .zip(pixel[..3].iter())
                .any(|(a, b)| a.abs_diff(*b) > 4)
            {
                return false;
            }
        }
    }
    found
}

fn crop_capture(bits: &[u8], source_width: usize, window: &WindowInfo) -> RgbaImage {
    let source_height = bits.len() / 4 / source_width;
    let offset_x = window.frame.left - window.raw.left;
    let offset_y = window.frame.top - window.raw.top;
    let target_width = window.frame.width() as usize;
    let target_height = window.frame.height() as usize;

    let mut image = RgbaImage::new(target_width as u32, target_height as u32);
    for y in 0..target_height {
        let source_y = offset_y + y as i32;
        if source_y < 0 || source_y as usize >= source_height {
            continue;
        }
        for x in 0..target_width {
            let source_x = offset_x + x as i32;
            if source_x < 0 || source_x as usize >= source_width {
                continue;
            }
            let index = (source_y as usize * source_width + source_x as usize) * 4;
            image.put_pixel(
                x as u32,
                y as u32,
                Rgba([
                    bits[index],
                    bits[index + 1],
                    bits[index + 2],
                    bits[index + 3],
                ]),
            );
        }
    }
    image
}
