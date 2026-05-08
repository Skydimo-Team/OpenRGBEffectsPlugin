mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const ENDPOINT_EPSILON: f32 = 0.0005;
const HALF_WIDTH: f32 = 2.0;

#[derive(Clone, Copy)]
struct MotionPointConfig {
    speed: f32,
    random_enabled: bool,
    color: SkydimoRgb,
    background: SkydimoRgb,
}

impl Default for MotionPointConfig {
    fn default() -> Self {
        Self {
            speed: 50.0,
            random_enabled: false,
            color: SkydimoRgb { r: 255, g: 0, b: 0 },
            background: SkydimoRgb { r: 0, g: 0, b: 0 },
        }
    }
}

struct MotionPointEffect {
    config: MotionPointConfig,
    progress: f32,
    last_t: Option<f64>,
    current_color: SkydimoRgb,
    was_at_endpoint: bool,
    width: usize,
    height: usize,
    row_cache: Vec<SkydimoRgb>,
    rng: XorShift64,
}

impl MotionPointEffect {
    fn new() -> Self {
        let config = MotionPointConfig::default();
        Self {
            config,
            progress: 0.0,
            last_t: None,
            current_color: config.color,
            was_at_endpoint: false,
            width: 0,
            height: 1,
            row_cache: Vec::new(),
            rng: XorShift64::seeded(),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = led_count.max(1) as usize;
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }

        let mut random_updated = false;
        if let Some(random_enabled) = json_bool(json, "random") {
            self.config.random_enabled = random_enabled;
            random_updated = true;
        }

        let mut color_updated = false;
        if let Some(color) = json_color(json, "color") {
            self.config.color = color;
            color_updated = true;
        }

        if let Some(background) = json_color(json, "background") {
            self.config.background = background;
        }

        if !self.config.random_enabled && (random_updated || color_updated) {
            self.current_color = self.config.color;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let sine_t = (1.0 + self.progress.sin()) * 0.5;
        self.update_current_color(sine_t);

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width.max(1)
        };
        self.render(sine_t, width, pixels);

        let dt = self.tick_delta(elapsed_seconds);
        if dt > 0.0 {
            self.progress += 0.05 * self.config.speed * dt;
        }
    }

    fn update_current_color(&mut self, sine_t: f32) {
        if !self.config.random_enabled {
            self.current_color = self.config.color;
            self.was_at_endpoint = false;
            return;
        }

        let at_endpoint = sine_t <= ENDPOINT_EPSILON || sine_t >= 1.0 - ENDPOINT_EPSILON;
        if at_endpoint && !self.was_at_endpoint {
            self.current_color = hsv_to_rgb(self.rng.next_f32() * 360.0);
        }
        self.was_at_endpoint = at_endpoint;
    }

    fn render(&mut self, sine_t: f32, width: usize, pixels: &mut [SkydimoRgb]) {
        if pixels.len() <= width {
            render_row(
                pixels,
                width,
                sine_t,
                self.current_color,
                self.config.background,
            );
            return;
        }

        self.row_cache.resize(width, SkydimoRgb::default());
        render_row(
            &mut self.row_cache,
            width,
            sine_t,
            self.current_color,
            self.config.background,
        );

        let mut offset = 0usize;
        while offset < pixels.len() {
            let copy_len = width.min(pixels.len() - offset);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.row_cache.as_ptr(),
                    pixels.as_mut_ptr().add(offset),
                    copy_len,
                );
            }
            offset += copy_len;
        }
    }

    fn tick_delta(&mut self, elapsed_seconds: f64) -> f32 {
        let dt = match self.last_t.replace(elapsed_seconds) {
            Some(last_t) => elapsed_seconds - last_t,
            None => 0.0,
        };

        if !(0.0..=0.5).contains(&dt) {
            0.0
        } else {
            dt as f32
        }
    }
}

unsafe extern "C" fn motion_point_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(MotionPointEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn motion_point_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<MotionPointEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn motion_point_resize(
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

unsafe extern "C" fn motion_point_update_params_json(
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
        let Ok(json) = std::str::from_utf8(bytes) else {
            return -2;
        };
        effect.update_params(json);
        0
    })
}

unsafe extern "C" fn motion_point_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if buffer.is_null() && len > 0 {
            return -2;
        }
        if len == 0 {
            return 0;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn motion_point_is_ready(instance: *mut c_void) -> i32 {
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
                    create: Some(motion_point_create),
                    destroy: Some(motion_point_destroy),
                    resize: Some(motion_point_resize),
                    update_params_json: Some(motion_point_update_params_json),
                    tick: Some(motion_point_tick),
                    is_ready: Some(motion_point_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut MotionPointEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<MotionPointEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn render_row(
    pixels: &mut [SkydimoRgb],
    width: usize,
    sine_t: f32,
    color: SkydimoRgb,
    background: SkydimoRgb,
) {
    let point_pos = sine_t * (width.saturating_sub(1) as f32);

    for (x, pixel) in pixels.iter_mut().enumerate() {
        let distance = (x as f32 - point_pos).abs();
        *pixel = if distance > HALF_WIDTH {
            background
        } else {
            let factor = distance / HALF_WIDTH;
            lerp_rgb(color, background, factor)
        };
    }
}

#[inline]
fn lerp_rgb(left: SkydimoRgb, right: SkydimoRgb, t: f32) -> SkydimoRgb {
    let inv = 1.0 - t;
    SkydimoRgb {
        r: to_u8(left.r as f32 * inv + right.r as f32 * t),
        g: to_u8(left.g as f32 * inv + right.g as f32 * t),
        b: to_u8(left.b as f32 * inv + right.b as f32 * t),
    }
}

#[inline]
fn to_u8(value: f32) -> u8 {
    (value + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn hsv_to_rgb(hue: f32) -> SkydimoRgb {
    let hue = hue.rem_euclid(360.0) / 60.0;
    let sector = hue.floor() as u32;
    let fraction = hue - sector as f32;
    let inverse = 1.0 - fraction;

    let (r, g, b) = match sector {
        0 => (1.0, fraction, 0.0),
        1 => (inverse, 1.0, 0.0),
        2 => (0.0, 1.0, fraction),
        3 => (0.0, inverse, 1.0),
        4 => (fraction, 0.0, 1.0),
        _ => (1.0, 0.0, inverse),
    };

    SkydimoRgb {
        r: to_u8(r * 255.0),
        g: to_u8(g * 255.0),
        b: to_u8(b * 255.0),
    }
}

fn json_number(json: &str, key: &str) -> Option<f32> {
    let mut raw = json_value_after_key(json, key)?;
    if let Some(rest) = raw.strip_prefix('"') {
        raw = rest;
    }
    let end = raw
        .char_indices()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(raw.len());
    raw[..end].trim().parse::<f32>().ok()
}

fn json_bool(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_after_key(json, key)?;
    if raw.starts_with("true") {
        Some(true)
    } else if raw.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_color(json: &str, key: &str) -> Option<SkydimoRgb> {
    let raw = json_value_after_key(json, key)?;
    let raw = raw.strip_prefix('"')?;
    let end = json_string_end(raw)?;
    parse_hex_color(&raw[..end])
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon_pos = after_key.find(':')?;
    Some(after_key[colon_pos + 1..].trim_start())
}

fn json_string_end(raw: &str) -> Option<usize> {
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(idx),
            _ => {}
        }
    }
    None
}

fn parse_hex_color(raw: &str) -> Option<SkydimoRgb> {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let bytes = hex.as_bytes();

    match bytes.len() {
        3 => Some(SkydimoRgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: parse_hex_byte(bytes[0], bytes[1])?,
            g: parse_hex_byte(bytes[2], bytes[3])?,
            b: parse_hex_byte(bytes[4], bytes[5])?,
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

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn seeded() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let mut state = nanos ^ 0xA076_1D64_78BD_642F;
        if state == 0 {
            state = 0x9E37_79B9_7F4A_7C15;
        }
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x.max(1);
        x
    }

    fn next_f32(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 16_777_216.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        hsv_to_rgb, json_bool, json_color, json_number, render_row, MotionPointEffect, SkydimoRgb,
    };

    #[test]
    fn parses_motion_point_params() {
        let json = r##"{"speed":75,"random":true,"color":"#0af","background":"#102030"}"##;

        assert_eq!(json_number(json, "speed"), Some(75.0));
        assert_eq!(json_bool(json, "random"), Some(true));
        assert_eq!(json_color(json, "color"), Some(SkydimoRgb { r: 0, g: 170, b: 255 }));
        assert_eq!(
            json_color(json, "background"),
            Some(SkydimoRgb {
                r: 16,
                g: 32,
                b: 48,
            })
        );
    }

    #[test]
    fn renders_center_point_with_two_led_falloff() {
        let mut row = [SkydimoRgb::default(); 5];
        render_row(
            &mut row,
            5,
            0.5,
            SkydimoRgb { r: 255, g: 0, b: 0 },
            SkydimoRgb { r: 0, g: 0, b: 0 },
        );

        assert_eq!(
            row,
            [
                SkydimoRgb { r: 0, g: 0, b: 0 },
                SkydimoRgb { r: 128, g: 0, b: 0 },
                SkydimoRgb { r: 255, g: 0, b: 0 },
                SkydimoRgb { r: 128, g: 0, b: 0 },
                SkydimoRgb { r: 0, g: 0, b: 0 },
            ]
        );
    }

    #[test]
    fn repeats_cached_row_across_matrix() {
        let mut effect = MotionPointEffect::new();
        effect.resize(5, 2, 10);

        let mut pixels = [SkydimoRgb::default(); 10];
        effect.tick(0.0, &mut pixels);

        assert_eq!(pixels[..5], pixels[5..]);
    }

    #[test]
    fn first_tick_does_not_advance_phase() {
        let mut effect = MotionPointEffect::new();
        let mut pixels = [SkydimoRgb::default(); 5];

        effect.tick(10.0, &mut pixels);

        assert_eq!(effect.progress, 0.0);
    }

    #[test]
    fn converts_hsv_primary_colors() {
        assert_eq!(hsv_to_rgb(0.0), SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsv_to_rgb(120.0), SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(hsv_to_rgb(240.0), SkydimoRgb { r: 0, g: 0, b: 255 });
    }
}
