use image::{Rgba, RgbaImage};

/// Logical-pixel style settings. `apply_style` scales them by the window's DPI.
pub struct Style {
    pub radius: f32,
    pub padding: f32,
    pub shadow_blur: f32,
    pub shadow_offset: f32,
    pub shadow_opacity: f32,
    pub shadow_color: [u8; 3],
}

impl Default for Style {
    fn default() -> Self {
        Self {
            radius: 12.0,
            padding: 28.0,
            shadow_blur: 18.0,
            shadow_offset: 6.0,
            shadow_opacity: 0.35,
            shadow_color: [0, 0, 0],
        }
    }
}

/// Composite the window onto a transparent canvas with antialiased rounded
/// corners and a soft, offset drop shadow.
pub fn apply_style(window: &RgbaImage, style: &Style, scale: f32) -> RgbaImage {
    let width = window.width() as usize;
    let height = window.height() as usize;

    let radius = (style.radius * scale).max(0.0);
    let shadow_blur = (style.shadow_blur * scale).max(0.0).round() as usize;
    let shadow_offset = (style.shadow_offset * scale).round() as i32;
    let requested_padding = (style.padding * scale).max(0.0).round() as usize;

    let box_radius = if shadow_blur > 0 {
        (shadow_blur / 3).max(1)
    } else {
        0
    };
    let shadow_spread = box_radius * 3;
    let padding = requested_padding.max(if shadow_blur > 0 {
        shadow_spread + shadow_offset.unsigned_abs() as usize + 2
    } else {
        0
    });

    let mut output = RgbaImage::from_pixel(
        (width + 2 * padding) as u32,
        (height + 2 * padding) as u32,
        Rgba([0, 0, 0, 0]),
    );

    let mask = if radius > 0.0 {
        rounded_mask(width, height, radius)
    } else {
        vec![255; width * height]
    };

    if shadow_blur > 0 && style.shadow_opacity > 0.0 {
        draw_shadow(
            &mut output,
            &mask,
            width,
            height,
            padding,
            shadow_offset,
            box_radius,
            style.shadow_opacity,
            style.shadow_color,
        );
    }

    for y in 0..height {
        for x in 0..width {
            let source = *window.get_pixel(x as u32, y as u32);
            let alpha = source.0[3] as f32 / 255.0 * mask[y * width + x] as f32 / 255.0;
            if alpha <= 0.0 {
                continue;
            }
            let destination = output.get_pixel_mut((x + padding) as u32, (y + padding) as u32);
            blend_over(destination, [source.0[0], source.0[1], source.0[2]], alpha);
        }
    }

    output
}

/// Signed-distance coverage of a rounded rectangle with antialiased edges.
fn rounded_mask(width: usize, height: usize, radius: f32) -> Vec<u8> {
    let mut mask = vec![0u8; width * height];
    let w = width as f32;
    let h = height as f32;
    let radius = radius.min(w.min(h) / 2.0);
    let half_width = w / 2.0;
    let half_height = h / 2.0;

    for y in 0..height {
        let py = y as f32 + 0.5 - half_height;
        for x in 0..width {
            let px = x as f32 + 0.5 - half_width;
            let qx = px.abs() - (half_width - radius);
            let qy = py.abs() - (half_height - radius);
            let outside = qx.max(0.0).hypot(qy.max(0.0));
            let inside = qx.max(qy).min(0.0);
            let distance = outside + inside - radius;
            let coverage = (0.5 - distance).clamp(0.0, 1.0);
            mask[y * width + x] = (coverage * 255.0).round() as u8;
        }
    }
    mask
}

fn draw_shadow(
    output: &mut RgbaImage,
    mask: &[u8],
    width: usize,
    height: usize,
    padding: usize,
    offset_y: i32,
    box_radius: usize,
    opacity: f32,
    color: [u8; 3],
) {
    let spread_width = width + 2 * box_radius;
    let spread_height = height + 2 * box_radius;
    let mut padded = vec![0u8; spread_width * spread_height];
    for y in 0..height {
        let source = &mask[y * width..(y + 1) * width];
        let destination = &mut padded[(y + box_radius) * spread_width + box_radius..];
        destination[..width].copy_from_slice(source);
    }

    let blurred = blur_alpha(&padded, spread_width, spread_height, box_radius, 3);
    let origin_x = padding as i32 - box_radius as i32;
    let origin_y = padding as i32 + offset_y - box_radius as i32;
    let output_width = output.width() as i32;
    let output_height = output.height() as i32;

    for y in 0..spread_height {
        let out_y = origin_y + y as i32;
        if out_y < 0 || out_y >= output_height {
            continue;
        }
        for x in 0..spread_width {
            let out_x = origin_x + x as i32;
            if out_x < 0 || out_x >= output_width {
                continue;
            }
            let alpha = blurred[y * spread_width + x] as f32 / 255.0 * opacity;
            if alpha <= 0.0 {
                continue;
            }
            let destination = output.get_pixel_mut(out_x as u32, out_y as u32);
            blend_over(destination, color, alpha);
        }
    }
}

/// Three box blurs approximate a Gaussian while staying dependency-free.
fn blur_alpha(source: &[u8], width: usize, height: usize, radius: usize, passes: usize) -> Vec<u8> {
    if radius == 0 || passes == 0 {
        return source.to_vec();
    }

    let mut current = source.to_vec();
    let mut horizontal = vec![0u8; current.len()];
    let mut next = vec![0u8; current.len()];
    let mut column_in = vec![0u8; height];
    let mut column_out = vec![0u8; height];

    for _ in 0..passes {
        for y in 0..height {
            let offset = y * width;
            box_blur_line(
                &current[offset..offset + width],
                &mut horizontal[offset..offset + width],
                radius,
            );
        }
        for x in 0..width {
            for y in 0..height {
                column_in[y] = horizontal[y * width + x];
            }
            box_blur_line(&column_in, &mut column_out, radius);
            for y in 0..height {
                next[y * width + x] = column_out[y];
            }
        }
        std::mem::swap(&mut current, &mut next);
    }
    current
}

fn box_blur_line(input: &[u8], output: &mut [u8], radius: usize) {
    let length = input.len();
    let mut left = 0usize;
    let mut right = 0usize;
    let mut sum = 0usize;

    for index in 0..length {
        let window_right = (index + radius).min(length - 1);
        while right <= window_right {
            sum += input[right] as usize;
            right += 1;
        }
        let window_left = index.saturating_sub(radius);
        while left < window_left {
            sum -= input[left] as usize;
            left += 1;
        }
        output[index] = (sum / (right - left)) as u8;
    }
}

fn blend_over(destination: &mut Rgba<u8>, color: [u8; 3], alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let destination_alpha = destination.0[3] as f32 / 255.0;
    let out_alpha = alpha + destination_alpha * (1.0 - alpha);
    if out_alpha <= 0.0 {
        *destination = Rgba([0, 0, 0, 0]);
        return;
    }

    for channel in 0..3 {
        let value = (color[channel] as f32 * alpha
            + destination.0[channel] as f32 * destination_alpha * (1.0 - alpha))
            / out_alpha;
        destination.0[channel] = value.round().clamp(0.0, 255.0) as u8;
    }
    destination.0[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}
