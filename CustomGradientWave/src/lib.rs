mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const CUSTOM_PRESET: u8 = 0;
const HORIZONTAL: u8 = 0;
const VERTICAL: u8 = 1;
const RADIAL_OUT: u8 = 2;
const RADIAL_IN: u8 = 3;
const GRADIENT_SAMPLES: usize = 100;
const DEFAULT_WIDTH: usize = 1;
const DEFAULT_HEIGHT: usize = 1;

const DEFAULT_CUSTOM_COLORS: [SkydimoRgb; 8] = [
    rgb(0xFF, 0x00, 0x00),
    rgb(0xFF, 0x00, 0xE6),
    rgb(0x00, 0x00, 0xFF),
    rgb(0x00, 0xB3, 0xFF),
    rgb(0x00, 0xFF, 0x51),
    rgb(0xEA, 0xFF, 0x00),
    rgb(0xFF, 0xB3, 0x00),
    rgb(0xFF, 0x00, 0x00),
];
const PRESET_BOREALIS: [SkydimoRgb; 6] = [
    rgb(0x14, 0xE8, 0x1E),
    rgb(0x00, 0xEA, 0x8D),
    rgb(0x01, 0x7E, 0xD5),
    rgb(0xB5, 0x3D, 0xFF),
    rgb(0x8D, 0x00, 0xC4),
    rgb(0x14, 0xE8, 0x1E),
];
const PRESET_OCEAN: [SkydimoRgb; 5] = [
    rgb(0x00, 0x00, 0x7F),
    rgb(0x00, 0x00, 0xFF),
    rgb(0x00, 0xFF, 0xFF),
    rgb(0x00, 0xAA, 0xFF),
    rgb(0x00, 0x00, 0x7F),
];
const PRESET_PINK_BLUE: [SkydimoRgb; 4] = [
    rgb(0xFE, 0x00, 0xC5),
    rgb(0x00, 0xC5, 0xFF),
    rgb(0x00, 0xC5, 0xFF),
    rgb(0xFE, 0x00, 0xC5),
];
const PRESET_PINK_GOLD: [SkydimoRgb; 4] = [
    rgb(0xFE, 0xE0, 0x00),
    rgb(0xFE, 0x00, 0xFE),
    rgb(0xFE, 0x00, 0xFE),
    rgb(0xFE, 0xE0, 0x00),
];
const PRESET_PULSE: [SkydimoRgb; 5] = [
    rgb(0xFF, 0x55, 0x00),
    rgb(0x00, 0x00, 0x00),
    rgb(0x00, 0x00, 0x00),
    rgb(0x00, 0x00, 0x00),
    rgb(0xFF, 0x55, 0x00),
];
const PRESET_PURPLE_ORANGE: [SkydimoRgb; 6] = [
    rgb(0xFF, 0x21, 0x00),
    rgb(0xAA, 0x00, 0xFF),
    rgb(0xAA, 0x00, 0xFF),
    rgb(0xFF, 0x21, 0x00),
    rgb(0xFF, 0x21, 0x00),
    rgb(0xFF, 0x21, 0x00),
];
const PRESET_LIGHT_BLUE_PURPLE: [SkydimoRgb; 4] = [
    rgb(0x03, 0xFF, 0xFA),
    rgb(0x55, 0x00, 0x7F),
    rgb(0x55, 0x00, 0x7F),
    rgb(0x03, 0xFF, 0xFA),
];
const PRESET_POLICE_BEACON: [SkydimoRgb; 5] = [
    rgb(0xFF, 0x00, 0x00),
    rgb(0x00, 0x00, 0xFF),
    rgb(0x00, 0x00, 0xFF),
    rgb(0xFF, 0x00, 0x00),
    rgb(0xFF, 0x00, 0x00),
];
const PRESET_SEABED: [SkydimoRgb; 5] = [
    rgb(0x00, 0xFF, 0x00),
    rgb(0x00, 0x32, 0xFF),
    rgb(0x00, 0x32, 0xFF),
    rgb(0x00, 0xFF, 0x00),
    rgb(0x00, 0xFF, 0x00),
];
const PRESET_SUNSET: [SkydimoRgb; 8] = [
    rgb(0xFF, 0x21, 0x00),
    rgb(0xAB, 0x00, 0x6D),
    rgb(0xC0, 0x1C, 0x52),
    rgb(0xD5, 0x37, 0x37),
    rgb(0xEA, 0x53, 0x1B),
    rgb(0xFF, 0x6E, 0x00),
    rgb(0xFF, 0x00, 0x00),
    rgb(0xFF, 0x21, 0x00),
];
const PRESET_VAPORWAVE: [SkydimoRgb; 6] = [
    rgb(0xFF, 0x71, 0xCE),
    rgb(0xB9, 0x67, 0xFF),
    rgb(0x01, 0xCD, 0xFE),
    rgb(0x05, 0xFF, 0xA1),
    rgb(0xFF, 0xFB, 0x96),
    rgb(0xFF, 0x71, 0xCE),
];

#[derive(Clone, Copy, Debug, PartialEq)]
struct RadialCacheKey {
    width: usize,
    height: usize,
    led_count: usize,
    center_x_bits: u32,
    center_y_bits: u32,
}

struct CustomGradientWaveEffect {
    speed: f32,
    preset: u8,
    spread: f32,
    direction: u8,
    center_y_percent: f32,
    center_x_percent: f32,
    width: usize,
    height: usize,
    custom_colors: Vec<SkydimoRgb>,
    gradient: [SkydimoRgb; GRADIENT_SAMPLES],
    row_cache: Vec<SkydimoRgb>,
    radial_base_cache: Vec<f32>,
    radial_cache_key: Option<RadialCacheKey>,
}

impl CustomGradientWaveEffect {
    fn new() -> Self {
        let mut effect = Self {
            speed: 25.0,
            preset: 1,
            spread: 100.0,
            direction: HORIZONTAL,
            center_y_percent: 50.0,
            center_x_percent: 50.0,
            width: 0,
            height: DEFAULT_HEIGHT,
            custom_colors: DEFAULT_CUSTOM_COLORS.to_vec(),
            gradient: [SkydimoRgb::default(); GRADIENT_SAMPLES],
            row_cache: Vec::new(),
            radial_base_cache: Vec::new(),
            radial_cache_key: None,
        };
        effect.rebuild_gradient();
        effect
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = if width == 0 {
            (led_count as usize).max(DEFAULT_WIDTH)
        } else {
            width as usize
        };
        self.height = (height as usize).max(DEFAULT_HEIGHT);
        self.row_cache.clear();
        self.row_cache.reserve(self.width);
        self.radial_cache_key = None;
    }

    fn update_params_json(&mut self, bytes: &[u8]) {
        let mut rebuild_gradient = false;

        if let Some(speed) = json_number(bytes, b"speed") {
            self.speed = speed.clamp(1.0, 200.0);
        }
        if let Some(preset) = json_number(bytes, b"preset") {
            let next = rounded_u8(preset);
            if (next == CUSTOM_PRESET || preset_palette(next).is_some()) && self.preset != next {
                self.preset = next;
                rebuild_gradient = true;
            }
        }
        if let Some(colors) = json_color_array(bytes, b"colors") {
            self.custom_colors = if colors.len() < 2 {
                DEFAULT_CUSTOM_COLORS.to_vec()
            } else {
                colors
            };
            if self.preset == CUSTOM_PRESET {
                rebuild_gradient = true;
            }
        }
        if let Some(direction) = json_number(bytes, b"direction") {
            let next = rounded_u8(direction);
            if next <= RADIAL_IN {
                self.direction = next;
            }
        }
        if let Some(center_y) = json_number(bytes, b"height") {
            self.center_y_percent = center_y.clamp(0.0, 100.0);
            self.radial_cache_key = None;
        }
        if let Some(center_x) = json_number(bytes, b"width") {
            self.center_x_percent = center_x.clamp(0.0, 100.0);
            self.radial_cache_key = None;
        }
        if let Some(spread) = json_number(bytes, b"spread") {
            self.spread = spread.clamp(0.0, 100.0);
        }

        if rebuild_gradient {
            self.rebuild_gradient();
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        if pixels.is_empty() {
            return 0;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width
        }
        .max(DEFAULT_WIDTH);
        let height = self.height.max(DEFAULT_HEIGHT);
        let elapsed = if elapsed_seconds.is_finite() {
            elapsed_seconds.max(0.0) as f32
        } else {
            0.0
        };
        let progress = 0.01 * self.speed * elapsed;
        let spread_factor = self.spread * 0.01;

        match self.direction {
            HORIZONTAL => self.render_horizontal(width, height, spread_factor, progress, pixels),
            VERTICAL => self.render_vertical(width, height, spread_factor, progress, pixels),
            RADIAL_OUT => self.render_radial(width, height, spread_factor, progress, false, pixels),
            RADIAL_IN => self.render_radial(width, height, spread_factor, progress, true, pixels),
            _ => self.render_radial(width, height, spread_factor, progress, false, pixels),
        }

        0
    }

    fn render_horizontal(
        &mut self,
        width: usize,
        height: usize,
        spread_factor: f32,
        progress: f32,
        pixels: &mut [SkydimoRgb],
    ) {
        if self.row_cache.len() != width {
            self.row_cache.resize(width, SkydimoRgb::default());
        }

        let inv_width = 1.0 / width as f32;
        for (x, out) in self.row_cache.iter_mut().enumerate() {
            *out = sample_gradient_from(
                &self.gradient,
                spread_factor * x as f32 * inv_width + progress,
            );
        }
        self.copy_row_to_pixels(width, height, pixels);
    }

    fn render_vertical(
        &self,
        width: usize,
        height: usize,
        spread_factor: f32,
        progress: f32,
        pixels: &mut [SkydimoRgb],
    ) {
        let inv_height = 1.0 / height as f32;
        let mut offset = 0usize;
        for y in 0..height {
            if offset >= pixels.len() {
                break;
            }
            let color = sample_gradient_from(
                &self.gradient,
                spread_factor * y as f32 * inv_height + progress,
            );
            let take = width.min(pixels.len() - offset);
            pixels[offset..offset + take].fill(color);
            offset += take;
        }
        if offset < pixels.len() {
            pixels[offset..].fill(SkydimoRgb::default());
        }
    }

    fn render_radial(
        &mut self,
        width: usize,
        height: usize,
        spread_factor: f32,
        progress: f32,
        inward: bool,
        pixels: &mut [SkydimoRgb],
    ) {
        let rendered_len = width.saturating_mul(height).min(pixels.len());
        self.ensure_radial_cache(width, height, rendered_len);

        for (out, base) in pixels[..rendered_len]
            .iter_mut()
            .zip(self.radial_base_cache.iter().copied())
        {
            let position = if inward {
                (spread_factor * base + progress).abs()
            } else {
                (spread_factor * base - progress).abs()
            };
            *out = sample_gradient_from(&self.gradient, position);
        }
        if rendered_len < pixels.len() {
            pixels[rendered_len..].fill(SkydimoRgb::default());
        }
    }

    fn copy_row_to_pixels(&self, width: usize, height: usize, pixels: &mut [SkydimoRgb]) {
        let mut offset = 0usize;
        for _ in 0..height {
            if offset >= pixels.len() {
                break;
            }
            let take = width.min(pixels.len() - offset);
            pixels[offset..offset + take].copy_from_slice(&self.row_cache[..take]);
            offset += take;
        }
        if offset < pixels.len() {
            pixels[offset..].fill(SkydimoRgb::default());
        }
    }

    fn ensure_radial_cache(&mut self, width: usize, height: usize, led_count: usize) {
        let key = RadialCacheKey {
            width,
            height,
            led_count,
            center_x_bits: self.center_x_percent.to_bits(),
            center_y_bits: self.center_y_percent.to_bits(),
        };
        if self.radial_cache_key == Some(key) {
            return;
        }

        self.radial_base_cache.clear();
        self.radial_base_cache.reserve(led_count);
        let center_x = (width.saturating_sub(1) as f32) * (0.01 * self.center_x_percent);
        let center_y = (height.saturating_sub(1) as f32) * (0.01 * self.center_y_percent);
        let inv_width = 1.0 / width as f32;

        for y in 0..height {
            for x in 0..width {
                if self.radial_base_cache.len() == led_count {
                    self.radial_cache_key = Some(key);
                    return;
                }
                let dx = x as f32 - center_x;
                let dy = y as f32 - center_y;
                self.radial_base_cache
                    .push((dx.mul_add(dx, dy * dy)).sqrt() * inv_width);
            }
        }
        self.radial_cache_key = Some(key);
    }

    fn rebuild_gradient(&mut self) {
        let palette_storage;
        let palette: &[SkydimoRgb] = if let Some(palette) = preset_palette(self.preset) {
            palette
        } else {
            palette_storage = self.custom_colors.clone();
            palette_storage.as_slice()
        };
        match palette.len() {
            0 => {
                self.gradient.fill(SkydimoRgb::default());
                return;
            }
            1 => {
                self.gradient.fill(palette[0]);
                return;
            }
            _ => {}
        }

        let segment_count = (palette.len() - 1) as f32;
        for sample_index in 0..GRADIENT_SAMPLES {
            let position = (sample_index as f32 + 0.5) / GRADIENT_SAMPLES as f32;
            let scaled = position * segment_count;
            let scaled_floor = scaled.floor();
            let mut left_index = scaled_floor as usize;
            let mut blend = scaled - scaled_floor;

            if left_index >= palette.len() - 1 {
                left_index = palette.len() - 2;
                blend = 1.0;
            }
            self.gradient[sample_index] =
                lerp_rgb(palette[left_index], palette[left_index + 1], blend);
        }
    }

}

unsafe extern "C" fn custom_gradient_wave_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host = unsafe { &*host };
            if host.abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(CustomGradientWaveEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn custom_gradient_wave_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<CustomGradientWaveEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn custom_gradient_wave_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        effect.resize(width, height, led_count);
        0
    })
}

unsafe extern "C" fn custom_gradient_wave_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if ptr.is_null() || len == 0 {
            return 0;
        }

        let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
        effect.update_params_json(bytes);
        0
    })
}

unsafe extern "C" fn custom_gradient_wave_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if len == 0 {
            return 0;
        }
        if buffer.is_null() {
            return -2;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels)
    })
}

unsafe extern "C" fn custom_gradient_wave_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the native-c ABI declared in manifest.json.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    catch_ffi(|| {
        if out_api.is_null() || requested_abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
            return -1;
        }

        unsafe {
            *out_api = SkydimoPluginApiV1 {
                size: std::mem::size_of::<SkydimoPluginApiV1>() as u32,
                abi_version: SKYDIMO_NATIVE_C_ABI_VERSION,
                kind_mask: SKYDIMO_PLUGIN_KIND_EFFECT,
                effect: SkydimoEffectApiV1 {
                    size: std::mem::size_of::<SkydimoEffectApiV1>() as u32,
                    create: Some(custom_gradient_wave_create),
                    destroy: Some(custom_gradient_wave_destroy),
                    resize: Some(custom_gradient_wave_resize),
                    update_params_json: Some(custom_gradient_wave_update_params_json),
                    tick: Some(custom_gradient_wave_tick),
                    is_ready: Some(custom_gradient_wave_is_ready),
                },
                controller: SkydimoControllerApiV1 {
                    size: std::mem::size_of::<SkydimoControllerApiV1>() as u32,
                    ..SkydimoControllerApiV1::default()
                },
                extension: SkydimoExtensionApiV1 {
                    size: std::mem::size_of::<SkydimoExtensionApiV1>() as u32,
                    ..SkydimoExtensionApiV1::default()
                },
                shutdown_plugin: None,
            };
        }
        0
    })
}

fn effect_mut(instance: *mut c_void) -> Option<&'static mut CustomGradientWaveEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<CustomGradientWaveEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

const fn rgb(r: u8, g: u8, b: u8) -> SkydimoRgb {
    SkydimoRgb { r, g, b }
}

fn preset_palette(preset: u8) -> Option<&'static [SkydimoRgb]> {
    match preset {
        1 => Some(&DEFAULT_CUSTOM_COLORS),
        2 => Some(&PRESET_BOREALIS),
        3 => Some(&PRESET_OCEAN),
        4 => Some(&PRESET_PINK_BLUE),
        5 => Some(&PRESET_PINK_GOLD),
        6 => Some(&PRESET_PULSE),
        7 => Some(&PRESET_PURPLE_ORANGE),
        8 => Some(&PRESET_LIGHT_BLUE_PURPLE),
        9 => Some(&PRESET_POLICE_BEACON),
        10 => Some(&PRESET_SEABED),
        11 => Some(&PRESET_SUNSET),
        12 => Some(&PRESET_VAPORWAVE),
        _ => None,
    }
}

fn sample_gradient_from(gradient: &[SkydimoRgb; GRADIENT_SAMPLES], position: f32) -> SkydimoRgb {
    if !position.is_finite() {
        return SkydimoRgb::default();
    }
    let wrapped = position - position.floor();
    let index = ((GRADIENT_SAMPLES as f32 * wrapped).floor() as usize).min(GRADIENT_SAMPLES - 1);
    gradient[index]
}

fn lerp_rgb(left: SkydimoRgb, right: SkydimoRgb, blend: f32) -> SkydimoRgb {
    SkydimoRgb {
        r: lerp_channel(left.r, right.r, blend),
        g: lerp_channel(left.g, right.g, blend),
        b: lerp_channel(left.b, right.b, blend),
    }
}

fn lerp_channel(left: u8, right: u8, blend: f32) -> u8 {
    let value = left as f32 + (right as f32 - left as f32) * blend + 0.5;
    value.floor().clamp(0.0, 255.0) as u8
}

fn rounded_u8(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    value.clamp(0.0, u8::MAX as f32).round() as u8
}

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f32> {
    let mut pos = json_value_start(bytes, key)?;
    let quoted = bytes.get(pos).copied() == Some(b'"');
    if quoted {
        pos += 1;
    }

    let start = pos;
    while pos < bytes.len()
        && matches!(
            bytes[pos],
            b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'
        )
    {
        pos += 1;
    }
    if pos == start {
        return None;
    }
    if quoted && bytes.get(pos).copied() != Some(b'"') {
        return None;
    }

    std::str::from_utf8(&bytes[start..pos]).ok()?.parse().ok()
}

fn json_color_array(bytes: &[u8], key: &[u8]) -> Option<Vec<SkydimoRgb>> {
    let mut pos = json_value_start(bytes, key)?;
    if bytes.get(pos).copied()? != b'[' {
        return None;
    }
    pos += 1;

    let mut colors = Vec::new();
    while pos < bytes.len() {
        pos = skip_ascii_ws(bytes, pos);
        match bytes.get(pos).copied()? {
            b']' => return Some(colors),
            b',' => {
                pos += 1;
                continue;
            }
            b'"' => {
                pos += 1;
                let start = pos;
                let mut escaped = false;
                while pos < bytes.len() {
                    let byte = bytes[pos];
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        break;
                    }
                    pos += 1;
                }
                if pos >= bytes.len() {
                    return None;
                }
                if let Some(color) = parse_hex_color_bytes(&bytes[start..pos]) {
                    colors.push(color);
                }
                pos += 1;
            }
            _ => return None,
        }
    }

    None
}

fn json_value_start(bytes: &[u8], key: &[u8]) -> Option<usize> {
    let key_pos = find_json_key(bytes, key)?;
    let mut pos = key_pos + key.len() + 2;
    pos = skip_ascii_ws(bytes, pos);
    if bytes.get(pos).copied()? != b':' {
        return None;
    }
    Some(skip_ascii_ws(bytes, pos + 1))
}

fn find_json_key(bytes: &[u8], key: &[u8]) -> Option<usize> {
    if key.is_empty() || bytes.len() < key.len() + 2 {
        return None;
    }

    let last = bytes.len() - key.len() - 1;
    let mut pos = 0usize;
    while pos < last {
        if bytes[pos] == b'"'
            && bytes[pos + 1..].starts_with(key)
            && bytes.get(pos + key.len() + 1).copied() == Some(b'"')
        {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn skip_ascii_ws(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

fn parse_hex_color_bytes(value: &[u8]) -> Option<SkydimoRgb> {
    let mut compact = [0u8; 6];
    let mut len = 0usize;
    for byte in value.iter().copied().filter(|byte| !byte.is_ascii_whitespace()) {
        if byte == b'#' && len == 0 {
            continue;
        }
        if len == compact.len() {
            return None;
        }
        compact[len] = byte;
        len += 1;
    }

    match len {
        3 => Some(SkydimoRgb {
            r: parse_hex_nibble(compact[0])? * 17,
            g: parse_hex_nibble(compact[1])? * 17,
            b: parse_hex_nibble(compact[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: parse_hex_byte(compact[0], compact[1])?,
            g: parse_hex_byte(compact[2], compact[3])?,
            b: parse_hex_byte(compact[4], compact[5])?,
        }),
        _ => None,
    }
}

fn parse_hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((parse_hex_nibble(hi)? << 4) | parse_hex_nibble(lo)?)
}

fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::{
        custom_gradient_wave_create, custom_gradient_wave_destroy, custom_gradient_wave_tick,
        json_color_array, parse_hex_color_bytes, skydimo_plugin_get_api, CustomGradientWaveEffect,
    };
    use crate::abi::{
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_hex_colors_and_arrays() {
        assert_eq!(
            parse_hex_color_bytes(b"#0af"),
            Some(SkydimoRgb {
                r: 0,
                g: 170,
                b: 255
            })
        );
        assert_eq!(
            json_color_array(br##"{"colors":["#000000"," #ffffff "]}"##, b"colors"),
            Some(vec![
                SkydimoRgb { r: 0, g: 0, b: 0 },
                SkydimoRgb {
                    r: 255,
                    g: 255,
                    b: 255
                }
            ])
        );
    }

    #[test]
    fn rebuilds_gradient_with_lua_pixel_centers() {
        let effect = CustomGradientWaveEffect::new();
        assert_eq!(
            effect.gradient[0],
            SkydimoRgb {
                r: 255,
                g: 0,
                b: 8
            }
        );
    }

    #[test]
    fn applies_custom_palette_and_renders_horizontal_rows() {
        let mut effect = CustomGradientWaveEffect::new();
        effect.resize(4, 2, 8);
        effect.update_params_json(br##"{"preset":0,"colors":["#000000","#ffffff"]}"##);

        let mut pixels = vec![SkydimoRgb::default(); 8];
        assert_eq!(effect.tick(0.0, &mut pixels), 0);

        let expected = [
            SkydimoRgb { r: 1, g: 1, b: 1 },
            SkydimoRgb {
                r: 65,
                g: 65,
                b: 65,
            },
            SkydimoRgb {
                r: 129,
                g: 129,
                b: 129,
            },
            SkydimoRgb {
                r: 193,
                g: 193,
                b: 193,
            },
        ];
        assert_eq!(&pixels[..4], &expected);
        assert_eq!(&pixels[4..], &expected);
    }

    #[test]
    fn radial_render_reuses_distance_cache_until_layout_changes() {
        let mut effect = CustomGradientWaveEffect::new();
        effect.resize(3, 3, 9);
        effect.update_params_json(br#"{"direction":2}"#);
        let mut pixels = vec![SkydimoRgb::default(); 9];

        assert_eq!(effect.tick(0.0, &mut pixels), 0);
        let key = effect.radial_cache_key;
        assert!(key.is_some());
        assert_eq!(effect.tick(1.0, &mut pixels), 0);
        assert_eq!(effect.radial_cache_key, key);

        effect.update_params_json(br#"{"width":25}"#);
        assert_eq!(effect.tick(1.0, &mut pixels), 0);
        assert_ne!(effect.radial_cache_key, key);
    }

    #[test]
    fn exports_effect_api_for_current_abi() {
        let mut api = SkydimoPluginApiV1::default();
        let status = unsafe {
            skydimo_plugin_get_api(
                SKYDIMO_NATIVE_C_ABI_VERSION,
                std::ptr::null(),
                &mut api,
            )
        };

        assert_eq!(status, 0);
        assert_eq!(api.abi_version, SKYDIMO_NATIVE_C_ABI_VERSION);
        assert_eq!(api.kind_mask, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }

    #[test]
    fn ffi_create_tick_destroy_round_trip() {
        let mut instance: *mut c_void = std::ptr::null_mut();
        assert_eq!(
            unsafe { custom_gradient_wave_create(std::ptr::null(), &mut instance) },
            0
        );
        assert!(!instance.is_null());

        let mut pixels = [SkydimoRgb::default(); 4];
        assert_eq!(
            unsafe { custom_gradient_wave_tick(instance, 0.0, pixels.as_mut_ptr(), pixels.len()) },
            0
        );
        unsafe { custom_gradient_wave_destroy(instance) };

        assert_ne!(pixels[0], SkydimoRgb::default());
    }
}
