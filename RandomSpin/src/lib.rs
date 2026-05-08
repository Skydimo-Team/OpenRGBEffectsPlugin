use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const FPS: f32 = 60.0;
const GRADIENT_SIZE: usize = 100;

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
struct RandomSpinConfig {
    speed: f32,
    color1: SkydimoRgb,
    color2: SkydimoRgb,
}

impl Default for RandomSpinConfig {
    fn default() -> Self {
        Self {
            speed: 50.0,
            color1: SkydimoRgb { r: 255, g: 0, b: 0 },
            color2: SkydimoRgb { r: 0, g: 0, b: 255 },
        }
    }
}

struct RandomSpinEffect {
    config: RandomSpinConfig,
    gradient: [SkydimoRgb; GRADIENT_SIZE],
    progress: f32,
    entry_progress: f32,
    entry_stop_progress: f32,
    entry_speed_mult: f32,
    entry_dir: bool,
    entry_stop: bool,
    entry_next_time_point: f32,
    last_t: Option<f32>,
    frame_accumulator: f32,
    width: usize,
    height: usize,
    rng: XorShift64,
}

impl RandomSpinEffect {
    fn new() -> Self {
        let mut effect = Self {
            config: RandomSpinConfig::default(),
            gradient: [SkydimoRgb::default(); GRADIENT_SIZE],
            progress: 0.0,
            entry_progress: 0.0,
            entry_stop_progress: 0.0,
            entry_speed_mult: 1.0,
            entry_dir: true,
            entry_stop: true,
            entry_next_time_point: 0.0,
            last_t: None,
            frame_accumulator: 0.0,
            width: 0,
            height: 1,
            rng: XorShift64::seeded(),
        };
        effect.rebuild_gradient();
        effect
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }

        let mut colors_dirty = false;
        if let Some(colors) = json_color_pair(json, "colors") {
            if let Some(color) = colors[0] {
                if self.config.color1 != color {
                    self.config.color1 = color;
                    colors_dirty = true;
                }
            }
            if let Some(color) = colors[1] {
                if self.config.color2 != color {
                    self.config.color2 = color;
                    colors_dirty = true;
                }
            }
        }

        if colors_dirty {
            self.rebuild_gradient();
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = if self.width == 0 { pixels.len() } else { self.width.max(1) };
        let height = self.height.max(1);
        if height == 1 || width == 1 {
            self.render_linear(pixels);
        } else {
            self.render_matrix(pixels, width, height);
        }

        self.advance_state(elapsed_seconds as f32);
    }

    fn render_linear(&self, pixels: &mut [SkydimoRgb]) {
        let width_f = pixels.len().max(1) as f32;
        let offset = self.active_progress_offset();
        for (x, pixel) in pixels.iter_mut().enumerate() {
            *pixel = self.gradient_color_at(x as f32, width_f, offset);
        }
    }

    fn render_matrix(&self, pixels: &mut [SkydimoRgb], width: usize, height: usize) {
        let total = pixels.len().min(width.saturating_mul(height));
        let width_f = width as f32;
        let offset = self.active_progress_offset();
        let mut index = 0usize;

        for _ in 0..height {
            for x in 0..width {
                if index >= total {
                    if total < pixels.len() {
                        pixels[total..].fill(SkydimoRgb::default());
                    }
                    return;
                }
                pixels[index] = self.gradient_color_at(x as f32, width_f, offset);
                index += 1;
            }
        }

        if index < pixels.len() {
            pixels[index..].fill(SkydimoRgb::default());
        }
    }

    #[inline]
    fn active_progress_offset(&self) -> f32 {
        if self.entry_stop {
            self.entry_stop_progress.abs()
        } else {
            self.entry_progress.abs()
        }
    }

    #[inline]
    fn gradient_color_at(&self, x: f32, width: f32, offset: f32) -> SkydimoRgb {
        let percent = ((x / width) + offset).fract();
        let px = ((percent * GRADIENT_SIZE as f32) as usize).min(GRADIENT_SIZE - 1);
        self.gradient[px]
    }

    fn advance_state(&mut self, elapsed_seconds: f32) {
        let delta = if elapsed_seconds >= 0.0 {
            match self.last_t {
                None => elapsed_seconds.max(1.0 / FPS),
                Some(last) if elapsed_seconds < last => elapsed_seconds.max(1.0 / FPS),
                Some(last) => elapsed_seconds - last,
            }
        } else {
            0.0
        };
        self.last_t = Some(elapsed_seconds.max(0.0));

        if delta <= 0.0 {
            return;
        }

        self.frame_accumulator += delta * FPS;
        while self.frame_accumulator >= 1.0 {
            self.step_state();
            self.frame_accumulator -= 1.0;
        }
    }

    fn step_state(&mut self) {
        if self.entry_next_time_point < self.progress {
            self.entry_stop = !self.entry_stop;
            let max_time = if self.entry_stop { 1.5 } else { 3.5 };
            self.entry_next_time_point = self.progress + self.rng.range_f32(1.0, max_time);
            self.entry_speed_mult = self.rng.range_f32(1.0, 5.0);
            self.entry_dir = self.rng.next_bool();
            self.entry_stop_progress = self.entry_progress;
        } else {
            let dir_sign = if self.entry_dir { -1.0 } else { 1.0 };
            self.entry_progress += dir_sign * self.entry_speed_mult * 0.01 * self.config.speed / FPS;
        }

        self.progress += 0.01 * self.config.speed / FPS;
    }

    fn rebuild_gradient(&mut self) {
        const STOPS: [f32; 8] = [0.0, 0.15, 0.25, 0.35, 0.65, 0.75, 0.80, 0.85];
        let c1 = self.config.color1;
        let c2 = self.config.color2;
        let colors = [c1, c1, c2, c1, c1, c2, c1, c1];

        for x in 0..GRADIENT_SIZE {
            let t = x as f32 / GRADIENT_SIZE as f32;
            let mut color = colors[STOPS.len() - 1];

            if t <= STOPS[0] {
                color = colors[0];
            } else {
                for i in 1..STOPS.len() {
                    if t <= STOPS[i] {
                        let denom = (STOPS[i] - STOPS[i - 1]).max(f32::EPSILON);
                        let frac = (t - STOPS[i - 1]) / denom;
                        color = lerp_rgb(colors[i - 1], colors[i], frac);
                        break;
                    }
                }
            }

            self.gradient[x] = color;
        }
    }
}

unsafe extern "C" fn random_spin_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(RandomSpinEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn random_spin_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<RandomSpinEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn random_spin_resize(
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

unsafe extern "C" fn random_spin_update_params_json(
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

unsafe extern "C" fn random_spin_tick(
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

unsafe extern "C" fn random_spin_is_ready(instance: *mut c_void) -> i32 {
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
                    create: Some(random_spin_create),
                    destroy: Some(random_spin_destroy),
                    resize: Some(random_spin_resize),
                    update_params_json: Some(random_spin_update_params_json),
                    tick: Some(random_spin_tick),
                    is_ready: Some(random_spin_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut RandomSpinEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<RandomSpinEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn lerp_rgb(left: SkydimoRgb, right: SkydimoRgb, t: f32) -> SkydimoRgb {
    let inv = 1.0 - t;
    SkydimoRgb {
        r: to_u8(left.r as f32 * inv + right.r as f32 * t),
        g: to_u8(left.g as f32 * inv + right.g as f32 * t),
        b: to_u8(left.b as f32 * inv + right.b as f32 * t),
    }
}

fn to_u8(value: f32) -> u8 {
    (value + 0.5).floor().clamp(0.0, 255.0) as u8
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

fn json_color_pair(json: &str, key: &str) -> Option<[Option<SkydimoRgb>; 2]> {
    let raw = json_value_after_key(json, key)?;
    let mut raw = raw.strip_prefix('[')?;
    let mut colors = [None, None];

    for slot in &mut colors {
        raw = raw.trim_start();
        if let Some(rest) = raw.strip_prefix(']') {
            let _ = rest;
            break;
        }
        if let Some(rest) = raw.strip_prefix(',') {
            raw = rest.trim_start();
        }
        let rest = raw.strip_prefix('"')?;
        let end = json_string_end(rest)?;
        *slot = parse_hex_color(&rest[..end]);
        raw = &rest[end + 1..];
    }

    colors.iter().any(Option::is_some).then_some(colors)
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

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }

    fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

#[cfg(test)]
mod tests {
    use super::{json_color_pair, json_number, parse_hex_color, RandomSpinEffect, SkydimoRgb};

    #[test]
    fn parses_speed_numbers_and_strings() {
        assert_eq!(json_number(r#"{"speed":75}"#, "speed"), Some(75.0));
        assert_eq!(json_number(r#"{"speed":"25"}"#, "speed"), Some(25.0));
    }

    #[test]
    fn parses_color_pair() {
        let colors = json_color_pair(r##"{"colors":["#0af","#102030"]}"##, "colors").unwrap();
        assert_eq!(colors[0], Some(SkydimoRgb { r: 0, g: 170, b: 255 }));
        assert_eq!(colors[1], Some(SkydimoRgb { r: 16, g: 32, b: 48 }));
    }

    #[test]
    fn rejects_invalid_hex_color() {
        assert_eq!(parse_hex_color("#xyz"), None);
        assert_eq!(parse_hex_color("#12345"), None);
    }

    #[test]
    fn first_gradient_matches_default_color() {
        let effect = RandomSpinEffect::new();
        assert_eq!(effect.gradient[0], SkydimoRgb { r: 255, g: 0, b: 0 });
    }
}
