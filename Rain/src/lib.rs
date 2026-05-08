use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const FPS: f32 = 60.0;
const STEP_EPSILON: f64 = 1e-6;
const MAX_PALETTE_COLORS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct SkydimoRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
pub struct SkydimoHostApiV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoHardwareCandidateV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoDeviceInfoV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoOutputDefinitionV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoOutputFrameV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoLedColorV1 {
    _private: [u8; 0],
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
        unsafe extern "C" fn(
            *const SkydimoHostApiV1,
            *const SkydimoHardwareCandidateV1,
            *mut *mut c_void,
        ) -> i32,
    >,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub validate: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub init: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub get_device_info: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoDeviceInfoV1) -> i32>,
    pub get_output_count: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
    pub get_output:
        Option<unsafe extern "C" fn(*mut c_void, usize, *mut SkydimoOutputDefinitionV1) -> i32>,
    pub update: Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputFrameV1, usize) -> i32>,
    pub set_output_leds_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, usize) -> i32>,
    pub update_output:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputDefinitionV1) -> i32>,
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
            *const SkydimoOutputFrameV1,
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
struct RainConfig {
    speed: f32,
    max_drops: usize,
    drop_size: usize,
    random_enabled: bool,
    only_first_enabled: bool,
}

impl Default for RainConfig {
    fn default() -> Self {
        Self {
            speed: 25.0,
            max_drops: 20,
            drop_size: 1,
            random_enabled: false,
            only_first_enabled: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Drop {
    progress: f32,
    color: SkydimoRgb,
    col: usize,
    speed_mult: f32,
    size: usize,
}

struct RainEffect {
    config: RainConfig,
    palette: Vec<SkydimoRgb>,
    drops: Vec<Drop>,
    occupied: Vec<u8>,
    simulated_steps: u64,
    last_time: Option<f64>,
    width: usize,
    height: usize,
    last_width: usize,
    last_height: usize,
    rng: XorShift64,
}

impl RainEffect {
    fn new() -> Self {
        Self {
            config: RainConfig::default(),
            palette: default_palette().to_vec(),
            drops: Vec::with_capacity(50),
            occupied: Vec::new(),
            simulated_steps: 0,
            last_time: None,
            width: 0,
            height: 1,
            last_width: 0,
            last_height: 0,
            rng: XorShift64::seeded(),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback_width = (led_count as usize).max(1);
        self.width = if width == 0 { fallback_width } else { width as usize };
        self.height = if height == 0 { 1 } else { height as usize };
        self.height = self.height.max(1);
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = speed.round().clamp(1.0, 200.0);
        }
        if let Some(drops) = json_number(json, "drops") {
            self.config.max_drops = drops.round().clamp(1.0, 50.0) as usize;
        }
        if let Some(drop_size) = json_number(json, "drop_size") {
            self.config.drop_size = drop_size.round().clamp(1.0, 10.0) as usize;
        }
        if let Some(random_enabled) = json_bool(json, "random") {
            self.config.random_enabled = random_enabled;
        }
        if let Some(only_first_enabled) = json_bool(json, "only_first") {
            self.config.only_first_enabled = only_first_enabled;
        }
        if let Some(colors) = json_colors(json, "colors") {
            self.palette = colors;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = self.width.max(1);
        let height = self.height.max(1);
        let time_now = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            elapsed_seconds
        } else {
            0.0
        };
        let current_steps = frame_steps(time_now);

        if self
            .last_time
            .is_some_and(|last_time| time_now + STEP_EPSILON < last_time)
        {
            self.reset_state(current_steps);
        }

        if width != self.last_width || height != self.last_height {
            self.reset_state(current_steps);
            self.last_width = width;
            self.last_height = height;
        }

        self.sync_state(time_now, width, height);
        self.render(pixels, width, height);
        self.last_time = Some(time_now);
    }

    fn reset_state(&mut self, current_steps: u64) {
        self.drops.clear();
        self.simulated_steps = current_steps;
    }

    fn sync_state(&mut self, time_now: f64, width: usize, height: usize) {
        let target_steps = frame_steps(time_now);
        if target_steps < self.simulated_steps {
            self.reset_state(target_steps);
            return;
        }

        while self.simulated_steps < target_steps {
            self.step_once(width, height);
            self.simulated_steps += 1;
        }
    }

    fn step_once(&mut self, width: usize, height: usize) {
        self.trigger_drop(width);
        for drop in &mut self.drops {
            drop.progress += 0.5 * drop.speed_mult * self.config.speed / FPS;
        }
        let height = height as f32;
        self.drops
            .retain(|drop| drop.progress <= height + (3 * drop.size) as f32);
    }

    fn trigger_drop(&mut self, width: usize) {
        if width == 0 {
            return;
        }

        let max_drops = width.min(self.config.max_drops);
        if self.drops.len() >= max_drops {
            return;
        }

        let spawn_divisor = 2 + (FPS as usize / width);
        if self.rng.range_usize(spawn_divisor) != 0 {
            return;
        }

        let color = self.pick_drop_color();
        self.drops.push(Drop {
            progress: 0.0,
            color,
            col: self.rng.range_usize(width),
            speed_mult: self.rng.range_usize_inclusive(1, 3) as f32 + self.rng.next_f32(),
            size: self.config.drop_size,
        });
    }

    fn pick_drop_color(&mut self) -> SkydimoRgb {
        if self.config.only_first_enabled {
            return self.palette.first().copied().unwrap_or(SkydimoRgb { r: 255, g: 0, b: 0 });
        }

        if self.config.random_enabled {
            return hsv_to_rgb(self.rng.range_usize(360) as f32, 1.0, 1.0);
        }

        if self.palette.is_empty() {
            return SkydimoRgb { r: 255, g: 0, b: 0 };
        }
        self.palette[self.rng.range_usize(self.palette.len())]
    }

    fn render(&mut self, pixels: &mut [SkydimoRgb], width: usize, height: usize) {
        pixels.fill(SkydimoRgb::default());

        let active_len = width.saturating_mul(height).min(pixels.len());
        if active_len == 0 {
            return;
        }

        if self.occupied.len() != active_len {
            self.occupied.resize(active_len, 0);
        } else {
            self.occupied.fill(0);
        }

        let occupied = self.occupied.as_mut_slice();
        for drop in &self.drops {
            render_drop(drop, width, height, active_len, pixels, occupied);
        }
    }
}

fn render_drop(
    drop: &Drop,
    width: usize,
    height: usize,
    active_len: usize,
    pixels: &mut [SkydimoRgb],
    occupied: &mut [u8],
) {
    if width == 0 || height == 0 || drop.size == 0 {
        return;
    }

    let x_start = drop.col.saturating_sub(drop.size.saturating_sub(1));
    let x_end = drop.col.min(width - 1);
    if x_start > x_end {
        return;
    }

    let trail_length = trail_length(drop);
    let active_span = drop.size as f32 + 1.0 + trail_length as f32;
    let y_start = (drop.progress - active_span).ceil().max(0.0) as usize;
    let y_end_float = drop.progress.floor();
    if y_end_float < 0.0 {
        return;
    }
    let y_end = (y_end_float as usize).min(height - 1);
    if y_start > y_end {
        return;
    }

    for y in y_start..=y_end {
        let distance = drop.progress - y as f32;
        if distance < 0.0 || distance > active_span {
            continue;
        }

        let color = color_at_distance(drop, distance);
        let row = y.saturating_mul(width);
        if row >= active_len {
            break;
        }

        for x in x_start..=x_end {
            let index = row + x;
            if index >= active_len {
                break;
            }
            if occupied[index] == 0 {
                pixels[index] = color;
                occupied[index] = 1;
            }
        }
    }
}

#[inline]
fn trail_length(drop: &Drop) -> usize {
    ((drop.speed_mult - 1.0) * ((drop.size as f32 / 2.0) + 1.0))
        .floor()
        .max(0.0) as usize
}

#[inline]
fn color_at_distance(drop: &Drop, distance: f32) -> SkydimoRgb {
    let whole = distance.trunc();
    let frac = distance - whole;

    if whole == 0.0 {
        scale_color(drop.color, frac)
    } else if whole > 0.0 && whole <= drop.size as f32 {
        drop.color
    } else {
        scale_color(drop.color, 0.75 / (whole - drop.size as f32))
    }
}

#[inline]
fn scale_color(color: SkydimoRgb, factor: f32) -> SkydimoRgb {
    if factor <= 0.0 {
        return SkydimoRgb::default();
    }
    if factor >= 1.0 {
        return color;
    }

    SkydimoRgb {
        r: scale_channel(color.r, factor),
        g: scale_channel(color.g, factor),
        b: scale_channel(color.b, factor),
    }
}

#[inline]
fn scale_channel(value: u8, factor: f32) -> u8 {
    (value as f32 * factor).floor().clamp(0.0, 255.0) as u8
}

fn frame_steps(time_now: f64) -> u64 {
    ((time_now * FPS as f64) + STEP_EPSILON)
        .floor()
        .clamp(0.0, u64::MAX as f64) as u64
}

fn default_palette() -> [SkydimoRgb; 5] {
    [
        SkydimoRgb { r: 255, g: 0, b: 0 },
        SkydimoRgb { r: 255, g: 153, b: 0 },
        SkydimoRgb { r: 255, g: 255, b: 0 },
        SkydimoRgb { r: 0, g: 255, b: 136 },
        SkydimoRgb { r: 0, g: 170, b: 255 },
    ]
}

unsafe extern "C" fn rain_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(RainEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn rain_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<RainEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn rain_resize(
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

unsafe extern "C" fn rain_update_params_json(
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

unsafe extern "C" fn rain_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        let pixels = if len == 0 {
            &mut []
        } else {
            if buffer.is_null() {
                return -2;
            }
            unsafe { std::slice::from_raw_parts_mut(buffer, len) }
        };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn rain_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must be the native-c ABI declared in manifest.json.
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
                    create: Some(rain_create),
                    destroy: Some(rain_destroy),
                    resize: Some(rain_resize),
                    update_params_json: Some(rain_update_params_json),
                    tick: Some(rain_tick),
                    is_ready: Some(rain_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut RainEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<RainEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
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

fn json_colors(json: &str, key: &str) -> Option<Vec<SkydimoRgb>> {
    let mut raw = json_value_after_key(json, key)?;
    raw = raw.strip_prefix('[')?;
    let mut colors = Vec::with_capacity(5);

    loop {
        raw = raw.trim_start();
        if raw.starts_with(']') {
            break;
        }
        if let Some(rest) = raw.strip_prefix(',') {
            raw = rest.trim_start();
        }
        let Some(rest) = raw.strip_prefix('"') else {
            break;
        };
        let Some(end) = json_string_end(rest) else {
            break;
        };
        if let Some(color) = parse_hex_color(&rest[..end]) {
            colors.push(color);
            if colors.len() >= MAX_PALETTE_COLORS {
                break;
            }
        }
        raw = &rest[end + 1..];
    }

    (!colors.is_empty()).then_some(colors)
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

    if bytes.len() == 3 {
        return Some(SkydimoRgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        });
    }

    if bytes.len() != 6 {
        return None;
    }

    Some(SkydimoRgb {
        r: parse_hex_byte(bytes[0], bytes[1])?,
        g: parse_hex_byte(bytes[2], bytes[3])?,
        b: parse_hex_byte(bytes[4], bytes[5])?,
    })
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

    fn range_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }

    fn range_usize_inclusive(&mut self, min: usize, max: usize) -> usize {
        min + self.range_usize(max.saturating_sub(min) + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        color_at_distance, default_palette, frame_steps, hsv_to_rgb, json_bool, json_colors,
        json_number, parse_hex_color, render_drop, Drop, RainEffect, SkydimoRgb,
    };

    #[test]
    fn parses_params_without_allocating_json_values() {
        let json = r##"{"speed":87.4,"drops":12,"drop_size":3,"random":true,"only_first":false,"colors":["#0af","#102030"]}"##;
        assert_eq!(json_number(json, "speed"), Some(87.4));
        assert_eq!(json_number(json, "drops"), Some(12.0));
        assert_eq!(json_number(json, "drop_size"), Some(3.0));
        assert_eq!(json_bool(json, "random"), Some(true));
        assert_eq!(json_bool(json, "only_first"), Some(false));
        assert_eq!(
            json_colors(json, "colors").unwrap(),
            vec![
                SkydimoRgb { r: 0, g: 170, b: 255 },
                SkydimoRgb { r: 16, g: 32, b: 48 }
            ]
        );
    }

    #[test]
    fn parses_short_and_full_hex_colors() {
        assert_eq!(parse_hex_color("#0af"), Some(SkydimoRgb { r: 0, g: 170, b: 255 }));
        assert_eq!(
            parse_hex_color("#102030"),
            Some(SkydimoRgb { r: 16, g: 32, b: 48 })
        );
        assert_eq!(parse_hex_color("#xyz"), None);
    }

    #[test]
    fn hsv_matches_core_host_conversion_for_primary_hues() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), SkydimoRgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn scales_drop_head_and_tail_like_lua_modf_path() {
        let drop = Drop {
            progress: 0.0,
            color: SkydimoRgb { r: 100, g: 50, b: 25 },
            col: 0,
            speed_mult: 2.0,
            size: 1,
        };
        assert_eq!(color_at_distance(&drop, 0.5), SkydimoRgb { r: 50, g: 25, b: 12 });
        assert_eq!(color_at_distance(&drop, 1.0), drop.color);
        assert_eq!(color_at_distance(&drop, 2.0), SkydimoRgb { r: 75, g: 37, b: 18 });
    }

    #[test]
    fn render_drop_marks_first_drop_as_owning_even_when_head_is_black() {
        let first = Drop {
            progress: 0.0,
            color: SkydimoRgb { r: 255, g: 0, b: 0 },
            col: 0,
            speed_mult: 1.0,
            size: 1,
        };
        let second = Drop {
            progress: 1.0,
            color: SkydimoRgb { r: 0, g: 255, b: 0 },
            col: 0,
            speed_mult: 1.0,
            size: 1,
        };
        let mut pixels = [SkydimoRgb::default(); 1];
        let mut occupied = [0u8; 1];

        render_drop(&first, 1, 1, 1, &mut pixels, &mut occupied);
        render_drop(&second, 1, 1, 1, &mut pixels, &mut occupied);

        assert_eq!(pixels[0], SkydimoRgb::default());
        assert_eq!(occupied[0], 1);
    }

    #[test]
    fn effect_resets_on_time_rollback_and_dimension_change() {
        let mut effect = RainEffect::new();
        effect.palette = default_palette().to_vec();
        effect.resize(4, 1, 4);
        effect.drops.push(Drop {
            progress: 1.0,
            color: SkydimoRgb { r: 255, g: 0, b: 0 },
            col: 0,
            speed_mult: 1.0,
            size: 1,
        });
        effect.simulated_steps = frame_steps(1.0);
        effect.last_time = Some(1.0);

        let mut pixels = [SkydimoRgb::default(); 4];
        effect.tick(0.5, &mut pixels);
        assert!(effect.drops.is_empty());

        effect.drops.push(Drop {
            progress: 1.0,
            color: SkydimoRgb { r: 255, g: 0, b: 0 },
            col: 0,
            speed_mult: 1.0,
            size: 1,
        });
        effect.resize(2, 2, 4);
        effect.tick(0.5, &mut pixels);
        assert!(effect.drops.is_empty());
    }
}
