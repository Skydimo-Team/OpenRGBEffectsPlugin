use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const FPS: f32 = 60.0;

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
struct RandomMarqueeConfig {
    speed: f32,
    random_enabled: bool,
    color: SkydimoRgb,
}

impl Default for RandomMarqueeConfig {
    fn default() -> Self {
        Self {
            speed: 50.0,
            random_enabled: false,
            color: SkydimoRgb { r: 255, g: 0, b: 0 },
        }
    }
}

struct RandomMarqueeEffect {
    config: RandomMarqueeConfig,
    progress: f32,
    last_progress: i32,
    spacing: u32,
    speed_mult: f32,
    progress_mult: f32,
    reverse: bool,
    random_hue: u16,
    random_color: SkydimoRgb,
    last_elapsed: Option<f32>,
    frame_accumulator: f32,
    width: usize,
    height: usize,
    rng: XorShift64,
}

impl RandomMarqueeEffect {
    fn new() -> Self {
        Self {
            config: RandomMarqueeConfig::default(),
            progress: 0.0,
            last_progress: 0,
            spacing: 1,
            speed_mult: 0.5,
            progress_mult: 0.5,
            reverse: false,
            random_hue: 0,
            random_color: hsv_to_rgb(0.0, 1.0, 1.0),
            last_elapsed: None,
            frame_accumulator: 0.0,
            width: 0,
            height: 1,
            rng: XorShift64::seeded(),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = speed.clamp(1.0, 200.0);
        }
        if let Some(random_enabled) = json_bool(json, "random") {
            self.config.random_enabled = random_enabled;
        }
        if let Some(color) = json_string(json, "color").and_then(parse_hex_color) {
            self.config.color = color;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width.max(1)
        };
        let height = self.height.max(1);
        let render_len = pixels.len().min(width.saturating_mul(height));

        if render_len == 0 {
            self.advance_state(elapsed_seconds as f32);
            return;
        }

        let row_len = width.min(render_len);
        let color = self.active_color();
        self.render_row(&mut pixels[..row_len], color);

        let mut filled = row_len;
        while filled < render_len {
            let copy_len = row_len.min(render_len - filled);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pixels.as_ptr(),
                    pixels.as_mut_ptr().add(filled),
                    copy_len,
                );
            }
            filled += copy_len;
        }

        if render_len < pixels.len() {
            pixels[render_len..].fill(SkydimoRgb::default());
        }

        self.advance_state(elapsed_seconds as f32);
    }

    fn render_row(&self, row: &mut [SkydimoRgb], color: SkydimoRgb) {
        for (x, pixel) in row.iter_mut().enumerate() {
            *pixel = if self.is_lit_column(x as u32) {
                color
            } else {
                SkydimoRgb::default()
            };
        }
    }

    #[inline]
    fn active_color(&self) -> SkydimoRgb {
        if self.config.random_enabled {
            self.random_color
        } else {
            self.config.color
        }
    }

    #[inline]
    fn is_lit_column(&self, x: u32) -> bool {
        let direction = if self.reverse { -1.0 } else { 1.0 };
        let shift = trunc_toward_zero(direction * 20.0 * self.progress * self.speed_mult);
        let modulus = 2 * self.spacing.max(1);
        x.wrapping_add(shift as u32).is_multiple_of(modulus)
    }

    fn advance_state(&mut self, elapsed_seconds: f32) {
        let delta = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            match self.last_elapsed {
                None => elapsed_seconds.max(1.0 / FPS),
                Some(last) if elapsed_seconds < last => elapsed_seconds.max(1.0 / FPS),
                Some(last) => elapsed_seconds - last,
            }
        } else {
            0.0
        };

        if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            self.last_elapsed = Some(elapsed_seconds);
        }
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
        self.progress += 0.005 * self.config.speed * self.progress_mult / FPS;

        let current_progress = self.progress.floor() as i32;
        if self.last_progress != current_progress {
            self.last_progress = current_progress;
            self.speed_mult = self.rng.range_f32(0.5, 1.5);
            self.progress_mult = self.rng.range_f32(0.5, 1.5);
            self.reverse = self.rng.next_bool();
            self.spacing = 1 + self.rng.range_u32_inclusive(0, 2);

            if self.config.random_enabled {
                self.random_hue = self.rng.range_u32_inclusive(0, 359) as u16;
                self.random_color = hsv_to_rgb(self.random_hue as f32, 1.0, 1.0);
            }
        }
    }
}

unsafe extern "C" fn random_marquee_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(RandomMarqueeEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn random_marquee_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<RandomMarqueeEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn random_marquee_resize(
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

unsafe extern "C" fn random_marquee_update_params_json(
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

unsafe extern "C" fn random_marquee_tick(
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

unsafe extern "C" fn random_marquee_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
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
                    create: Some(random_marquee_create),
                    destroy: Some(random_marquee_destroy),
                    resize: Some(random_marquee_resize),
                    update_params_json: Some(random_marquee_update_params_json),
                    tick: Some(random_marquee_tick),
                    is_ready: Some(random_marquee_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut RandomMarqueeEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<RandomMarqueeEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn json_number(json: &str, key: &str) -> Option<f32> {
    let mut raw = json_value_after_key(json, key)?.trim_start();
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
    let mut raw = json_value_after_key(json, key)?.trim_start();
    if let Some(rest) = raw.strip_prefix('"') {
        raw = rest;
    }
    if raw.starts_with("true") {
        Some(true)
    } else if raw.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = json_value_after_key(json, key)?.trim_start();
    let raw = raw.strip_prefix('"')?;
    let end = json_string_end(raw)?;
    Some(&raw[..end])
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let mut start = 0usize;
    loop {
        let rel = json[start..].find('"')?;
        let quote = start + rel;
        let after_quote = quote + 1;
        let key_end = after_quote.checked_add(key.len())?;
        if json.get(after_quote..key_end) == Some(key)
            && json.as_bytes().get(key_end) == Some(&b'"')
        {
            let after_key = &json[key_end + 1..];
            let colon = after_key.find(':')?;
            return Some(&after_key[colon + 1..]);
        }
        start = after_quote;
    }
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

fn trunc_toward_zero(value: f32) -> i32 {
    if value >= 0.0 {
        value.floor() as i32
    } else {
        value.ceil() as i32
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
        Self::new(nanos ^ 0xA076_1D64_78BD_642F)
    }

    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
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

    fn range_u32_inclusive(&mut self, min: u32, max: u32) -> u32 {
        min + (self.next_u64() % u64::from(max - min + 1)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::{
        hsv_to_rgb, json_bool, json_number, parse_hex_color, RandomMarqueeEffect, SkydimoRgb,
        XorShift64, FPS,
    };

    #[test]
    fn parses_manifest_params() {
        let json = r##"{"speed":125,"random":true,"color":"#0af"}"##;
        assert_eq!(json_number(json, "speed"), Some(125.0));
        assert_eq!(json_bool(json, "random"), Some(true));
        assert_eq!(
            parse_hex_color("#0af"),
            Some(SkydimoRgb {
                r: 0,
                g: 170,
                b: 255,
            })
        );
    }

    #[test]
    fn hsv_matches_core_host_conversion_for_primary_hues() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), SkydimoRgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn render_repeats_first_row_across_matrix() {
        let mut effect = RandomMarqueeEffect::new();
        effect.rng = XorShift64::new(1);
        effect.resize(6, 2, 12);
        let mut pixels = vec![SkydimoRgb::default(); 12];
        effect.tick(1.0 / FPS as f64, &mut pixels);
        assert_eq!(&pixels[..6], &pixels[6..12]);
        assert_eq!(pixels[0], SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(pixels[1], SkydimoRgb::default());
    }

    #[test]
    fn reverse_motion_uses_u32_wrapping() {
        let mut effect = RandomMarqueeEffect::new();
        effect.reverse = true;
        effect.progress = 1.0;
        effect.speed_mult = 1.0;
        effect.spacing = 1;
        assert!(effect.is_lit_column(20));
        assert!(!effect.is_lit_column(19));
    }
}
