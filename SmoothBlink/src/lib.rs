use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const PI: f32 = std::f32::consts::PI;

type HostLogFn = unsafe extern "C" fn(*mut c_void, u32, *const c_char, usize);
type HostCallJsonFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    usize,
    *const c_char,
    usize,
    *mut u8,
    usize,
    *mut usize,
) -> i32;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoStr {
    pub ptr: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct SkydimoRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SkydimoHostApiV1 {
    pub size: u32,
    pub abi_version: u32,
    pub host_ctx: *mut c_void,
    pub log: Option<HostLogFn>,
    pub call_json: Option<HostCallJsonFn>,
    pub controller_set_device_info: Option<unsafe extern "C" fn(*mut c_void, *const c_void) -> i32>,
    pub controller_add_output: Option<unsafe extern "C" fn(*mut c_void, *const c_void) -> i32>,
    pub controller_output_led_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> usize>,
    pub controller_get_rgb_bytes:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *mut u8, usize) -> isize>,
    pub controller_write: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> isize>,
    pub controller_read: Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, u32) -> isize>,
    pub controller_hid_send_feature_report:
        Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> isize>,
    pub controller_hid_get_feature_report:
        Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, u8) -> isize>,
    pub extension_lock_leds: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const usize,
            usize,
            *mut usize,
            *mut usize,
        ) -> i32,
    >,
    pub extension_unlock_leds: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const usize,
            usize,
        ) -> i32,
    >,
    pub extension_set_leds_rgb: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const c_void,
            usize,
        ) -> i32,
    >,
    pub effect_audio_capture: Option<unsafe extern "C" fn(*mut c_void, usize, *mut c_void) -> i32>,
    pub effect_screen_capture:
        Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut c_void) -> i32>,
    pub effect_album_art:
        Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut c_void) -> i32>,
    pub get_plugin_id: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoStr) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoEffectApiV1 {
    pub size: u32,
    pub create: Option<unsafe extern "C" fn(*const SkydimoHostApiV1, *mut *mut c_void) -> i32>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub resize: Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> i32>,
    pub update_params_json: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> i32>,
    pub tick: Option<unsafe extern "C" fn(*mut c_void, f64, *mut SkydimoRgb, usize) -> i32>,
    pub is_ready: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoControllerApiV1 {
    pub size: u32,
    pub create: Option<
        unsafe extern "C" fn(*const SkydimoHostApiV1, *const c_void, *mut *mut c_void) -> i32,
    >,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub validate: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub init: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub get_device_info: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32>,
    pub get_output_count: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
    pub get_output: Option<unsafe extern "C" fn(*mut c_void, usize, *mut c_void) -> i32>,
    pub update: Option<unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> i32>,
    pub set_output_leds_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, usize) -> i32>,
    pub update_output: Option<unsafe extern "C" fn(*mut c_void, *const c_void) -> i32>,
    pub disconnect: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoExtensionApiV1 {
    pub size: u32,
    pub create: Option<unsafe extern "C" fn(*const SkydimoHostApiV1, *mut *mut c_void) -> i32>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub start: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub stop: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub on_scan_devices: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub on_event_json:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *const c_char, usize) -> i32>,
    pub on_page_message_json: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> i32>,
    pub on_device_frame: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_void,
            usize,
        ) -> i32,
    >,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoPluginApiV1 {
    pub size: u32,
    pub abi_version: u32,
    pub kind_mask: u32,
    pub effect: SkydimoEffectApiV1,
    pub controller: SkydimoControllerApiV1,
    pub extension: SkydimoExtensionApiV1,
    pub shutdown_plugin: Option<unsafe extern "C" fn()>,
}

#[derive(Clone, Copy)]
struct Config {
    interval: f32,
    pulses: f32,
    pulse_duration: f32,
    strength: f32,
    rendering: u32,
    cx_shift: f32,
    cy_shift: f32,
    random_enabled: bool,
    color1: SkydimoRgb,
    color2: SkydimoRgb,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval: 2.0,
            pulses: 2.0,
            pulse_duration: 0.5,
            strength: 0.5,
            rendering: 0,
            cx_shift: 50.0,
            cy_shift: 50.0,
            random_enabled: false,
            color1: SkydimoRgb { r: 0, g: 0, b: 0 },
            color2: SkydimoRgb { r: 255, g: 0, b: 0 },
        }
    }
}

struct SmoothBlinkEffect {
    config: Config,
    width: usize,
    height: usize,
    rng: XorShift64,
    random_color1: SkydimoRgb,
    random_color2: SkydimoRgb,
    next_color1: SkydimoRgb,
    next_color2: SkydimoRgb,
    last_cycle: i64,
}

impl SmoothBlinkEffect {
    fn new() -> Self {
        let mut rng = XorShift64::seeded();
        Self {
            config: Config::default(),
            width: 0,
            height: 1,
            random_color1: random_rgb(&mut rng),
            random_color2: random_rgb(&mut rng),
            next_color1: random_rgb(&mut rng),
            next_color2: random_rgb(&mut rng),
            rng,
            last_cycle: -1,
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(value) = json_number(json, "interval") {
            self.config.interval = value.max(0.001);
        }
        if let Some(value) = json_number(json, "pulses") {
            self.config.pulses = value.max(1.0);
        }
        if let Some(value) = json_number(json, "pulse_duration") {
            self.config.pulse_duration = value.max(0.001);
        }
        if let Some(value) = json_number(json, "strength") {
            self.config.strength = (value / 100.0).clamp(0.0, 1.0);
        }
        if let Some(value) = json_number(json, "rendering") {
            self.config.rendering = if value.round() >= 1.0 { 1 } else { 0 };
        }
        if let Some(value) = json_number(json, "cx") {
            self.config.cx_shift = value.clamp(0.0, 100.0);
        }
        if let Some(value) = json_number(json, "cy") {
            self.config.cy_shift = value.clamp(0.0, 100.0);
        }
        if let Some(value) = json_bool(json, "random") {
            self.config.random_enabled = value;
        }
        if let Some(value) = json_string(json, "color1") {
            self.config.color1 = hex_to_rgb(value);
        }
        if let Some(value) = json_string(json, "color2") {
            self.config.color2 = hex_to_rgb(value);
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let elapsed = elapsed_seconds.max(0.0) as f32;
        let pulses_total_duration = self.config.pulses * self.config.pulse_duration;
        let total_effect_duration = self.config.interval + pulses_total_duration;
        let cycle = (elapsed / total_effect_duration).floor() as i64;
        let time_in_cycle = elapsed - cycle as f32 * total_effect_duration;

        if cycle != self.last_cycle {
            self.last_cycle = cycle;
            self.random_color1 = self.next_color1;
            self.random_color2 = self.next_color2;
            self.next_color1 = random_rgb(&mut self.rng);
            self.next_color2 = random_rgb(&mut self.rng);
        }

        let (cur1, cur2) = self.current_colors(time_in_cycle);
        let value = self.blink_value(time_in_cycle, pulses_total_duration);

        if self.config.rendering == 1 {
            self.render_circle(pixels, cur1, cur2, value);
        } else {
            pixels.fill(lerp_rgb(cur1, cur2, value));
        }
    }

    fn current_colors(&self, time_in_cycle: f32) -> (SkydimoRgb, SkydimoRgb) {
        if !self.config.random_enabled {
            return (self.config.color1, self.config.color2);
        }

        let half_interval = 0.5 * self.config.interval;
        if half_interval > 0.0 && time_in_cycle <= half_interval {
            let fade = time_in_cycle / half_interval;
            (
                lerp_rgb(self.random_color1, self.next_color1, fade),
                lerp_rgb(self.random_color2, self.next_color2, fade),
            )
        } else {
            (self.next_color1, self.next_color2)
        }
    }

    fn blink_value(&self, time_in_cycle: f32, pulses_total_duration: f32) -> f32 {
        if time_in_cycle < self.config.interval {
            return 1.0;
        }

        let x = time_in_cycle - self.config.interval;
        let y = 0.5
            * (1.0
                + ((2.0 * self.config.pulses / pulses_total_duration) * x * PI - 0.5 * PI)
                    .sin());
        let s = 0.5 + (1.0 - self.config.strength) * 0.5;
        y - (y - s) / s
    }

    fn render_circle(
        &self,
        pixels: &mut [SkydimoRgb],
        color1: SkydimoRgb,
        color2: SkydimoRgb,
        value: f32,
    ) {
        if value >= 1.0 {
            pixels.fill(color2);
            return;
        }

        let width = if self.width == 0 { pixels.len() } else { self.width.max(1) };
        let height = self.height.max(1);
        let cx_mult = self.config.cx_shift / 100.0;
        let cy_mult = self.config.cy_shift / 100.0;

        if height <= 1 {
            let cx = width.saturating_sub(1) as f32 * cx_mult;
            let max_distance = width as f32;
            let count = pixels.len().min(width);
            for (col, pixel) in pixels[..count].iter_mut().enumerate() {
                let distance_percent = if max_distance > 0.0 {
                    (cx - col as f32).abs() / max_distance
                } else {
                    0.0
                };
                *pixel = lerp_rgb(color1, color2, (value + distance_percent).min(1.0));
            }
            if count < pixels.len() {
                pixels[count..].fill(color2);
            }
            return;
        }

        let cx = width.saturating_sub(1) as f32 * cx_mult;
        let cy = height.saturating_sub(1) as f32 * cy_mult;
        let max_distance = (width + height) as f32;
        let total = pixels.len().min(width.saturating_mul(height));
        let mut index = 0usize;
        for row in 0..height {
            if index >= total {
                break;
            }
            for col in 0..width {
                if index >= total {
                    break;
                }
                let dx = cx - col as f32;
                let dy = cy - row as f32;
                let distance_percent = if max_distance > 0.0 {
                    dx.mul_add(dx, dy * dy).sqrt() / max_distance
                } else {
                    0.0
                };
                pixels[index] = lerp_rgb(color1, color2, (value + distance_percent).min(1.0));
                index += 1;
            }
        }
        if total < pixels.len() {
            pixels[total..].fill(color2);
        }
    }
}

unsafe extern "C" fn smooth_blink_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host = unsafe { &*host };
            if host.abi_version < SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }
        let effect = Box::new(SmoothBlinkEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn smooth_blink_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<SmoothBlinkEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn smooth_blink_resize(
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

unsafe extern "C" fn smooth_blink_update_params_json(
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

unsafe extern "C" fn smooth_blink_tick(
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

unsafe extern "C" fn smooth_blink_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the Core native-c ABI version.
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
                    create: Some(smooth_blink_create),
                    destroy: Some(smooth_blink_destroy),
                    resize: Some(smooth_blink_resize),
                    update_params_json: Some(smooth_blink_update_params_json),
                    tick: Some(smooth_blink_tick),
                    is_ready: Some(smooth_blink_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut SmoothBlinkEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<SmoothBlinkEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline]
fn random_rgb(rng: &mut XorShift64) -> SkydimoRgb {
    hsv_to_rgb(rng.next_f32() * 360.0, 1.0, 1.0)
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
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> SkydimoRgb {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    SkydimoRgb {
        r: to_u8((r + m) * 255.0),
        g: to_u8((g + m) * 255.0),
        b: to_u8((b + m) * 255.0),
    }
}

fn hex_to_rgb(raw: &str) -> SkydimoRgb {
    let mut hex = raw.trim();
    if let Some(stripped) = hex.strip_prefix('#') {
        hex = stripped;
    }

    if hex.len() == 3 {
        let bytes = hex.as_bytes();
        let Some(r) = parse_hex_nibble(bytes[0]).map(|v| v * 17) else {
            return SkydimoRgb::default();
        };
        let Some(g) = parse_hex_nibble(bytes[1]).map(|v| v * 17) else {
            return SkydimoRgb::default();
        };
        let Some(b) = parse_hex_nibble(bytes[2]).map(|v| v * 17) else {
            return SkydimoRgb::default();
        };
        return SkydimoRgb { r, g, b };
    }

    if hex.len() != 6 {
        return SkydimoRgb::default();
    }

    let bytes = hex.as_bytes();
    let Some(r) = parse_hex_byte(bytes[0], bytes[1]) else {
        return SkydimoRgb::default();
    };
    let Some(g) = parse_hex_byte(bytes[2], bytes[3]) else {
        return SkydimoRgb::default();
    };
    let Some(b) = parse_hex_byte(bytes[4], bytes[5]) else {
        return SkydimoRgb::default();
    };
    SkydimoRgb { r, g, b }
}

#[inline]
fn parse_hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((parse_hex_nibble(hi)? << 4) | parse_hex_nibble(lo)?)
}

#[inline]
fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
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
        json_number(json, key).map(|value| value != 0.0)
    }
}

fn json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = json_value_after_key(json, key)?;
    let raw = raw.strip_prefix('"')?;
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(&raw[..idx]),
            _ => {}
        }
    }
    None
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon_pos = after_key.find(':')?;
    Some(after_key[colon_pos + 1..].trim_start())
}

#[inline]
fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
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

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    #[inline]
    fn next_f32(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 16_777_216.0
    }
}

#[cfg(test)]
mod tests {
    use super::{hex_to_rgb, lerp_rgb, SkydimoRgb, SmoothBlinkEffect};

    #[test]
    fn parses_hex_colors_like_lua_version() {
        let full = hex_to_rgb("#FF8001");
        assert_eq!((full.r, full.g, full.b), (255, 128, 1));

        let short = hex_to_rgb("#0af");
        assert_eq!((short.r, short.g, short.b), (0, 170, 255));

        let invalid = hex_to_rgb("#not-rgb");
        assert_eq!((invalid.r, invalid.g, invalid.b), (0, 0, 0));
    }

    #[test]
    fn solid_render_fills_entire_buffer() {
        let mut effect = SmoothBlinkEffect::new();
        effect.update_params(
            r##"{"color1":"#000000","color2":"#FF0000","random":false,"rendering":0}"##,
        );
        let mut pixels = [SkydimoRgb::default(); 4];
        effect.tick(0.0, &mut pixels);
        assert!(pixels.iter().all(|pixel| *pixel == SkydimoRgb { r: 255, g: 0, b: 0 }));
    }

    #[test]
    fn lerp_rounds_to_nearest_channel() {
        let color = lerp_rgb(
            SkydimoRgb { r: 0, g: 0, b: 0 },
            SkydimoRgb { r: 255, g: 0, b: 0 },
            0.5,
        );
        assert_eq!(color.r, 128);
    }
}
