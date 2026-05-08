mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const FRAME_DT: f64 = 1.0 / 60.0;
const VALUE_SCALE: f32 = 0.01;

#[derive(Clone, Copy)]
struct Config {
    background: SkydimoRgb,
    thickness: u32,
    speed: u32,
    amplitude: u32,
    frequency: u32,
    freq_m1: u32,
    freq_m2: u32,
    freq_m3: u32,
    freq_m4: u32,
    freq_m5: u32,
    freq_m6: u32,
    freq_m7: u32,
    freq_m8: u32,
    freq_m9: u32,
    freq_m10: u32,
    freq_m11: u32,
    freq_m12: u32,
    random: bool,
    color: SkydimoRgb,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            background: rgb(0, 0, 0),
            thickness: 2,
            speed: 50,
            amplitude: 100,
            frequency: 100,
            freq_m1: 210,
            freq_m2: 450,
            freq_m3: 172,
            freq_m4: 112,
            freq_m5: 400,
            freq_m6: 222,
            freq_m7: 43,
            freq_m8: 500,
            freq_m9: 211,
            freq_m10: 150,
            freq_m11: 172,
            freq_m12: 6,
            random: false,
            color: rgb(255, 0, 0),
        }
    }
}

struct FractalMotionEffect {
    config: Config,
    progress: f32,
    random_tick: f32,
    last_time: Option<f64>,
    time_carry: f64,
    current_random: SkydimoRgb,
    next_random: SkydimoRgb,
    width: usize,
    height: usize,
    rng: XorShift64,
}

impl FractalMotionEffect {
    fn new() -> Self {
        Self::with_seed(seed_from_time())
    }

    fn with_seed(seed: u64) -> Self {
        let mut effect = Self {
            config: Config::default(),
            progress: 0.0,
            random_tick: 0.0,
            last_time: None,
            time_carry: 0.0,
            current_random: rgb(255, 0, 0),
            next_random: rgb(255, 0, 0),
            width: 0,
            height: 0,
            rng: XorShift64::new(seed),
        };
        effect.rng.next_u64();
        effect.rng.next_u64();
        effect.rng.next_u64();
        effect.reset_runtime_state();
        effect
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        if width == 0 || height == 0 {
            self.width = led_count.max(1) as usize;
            self.height = 1;
            return;
        }
        self.width = width as usize;
        self.height = height as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(thickness) = json_number(json, "thickness") {
            self.config.thickness = rounded_u32(thickness).clamp(2, 20);
        }
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = rounded_u32(speed).clamp(20, 200);
        }
        if let Some(amplitude) = json_number(json, "amplitude") {
            self.config.amplitude = rounded_u32(amplitude).clamp(1, 10000);
        }
        if let Some(frequency) = json_number(json, "frequency") {
            self.config.frequency = rounded_u32(frequency).clamp(1, 10000);
        }
        if let Some(freq_m1) = json_number(json, "freq_m1") {
            self.config.freq_m1 = rounded_u32(freq_m1).clamp(1, 10000);
        }
        if let Some(freq_m2) = json_number(json, "freq_m2") {
            self.config.freq_m2 = rounded_u32(freq_m2).clamp(1, 10000);
        }
        if let Some(freq_m3) = json_number(json, "freq_m3") {
            self.config.freq_m3 = rounded_u32(freq_m3).clamp(1, 10000);
        }
        if let Some(freq_m4) = json_number(json, "freq_m4") {
            self.config.freq_m4 = rounded_u32(freq_m4).clamp(1, 1000);
        }
        if let Some(freq_m5) = json_number(json, "freq_m5") {
            self.config.freq_m5 = rounded_u32(freq_m5).clamp(1, 10000);
        }
        if let Some(freq_m6) = json_number(json, "freq_m6") {
            self.config.freq_m6 = rounded_u32(freq_m6).clamp(1, 10000);
        }
        if let Some(freq_m7) = json_number(json, "freq_m7") {
            self.config.freq_m7 = rounded_u32(freq_m7).clamp(1, 100);
        }
        if let Some(freq_m8) = json_number(json, "freq_m8") {
            self.config.freq_m8 = rounded_u32(freq_m8).clamp(1, 10000);
        }
        if let Some(freq_m9) = json_number(json, "freq_m9") {
            self.config.freq_m9 = rounded_u32(freq_m9).clamp(1, 10000);
        }
        if let Some(freq_m10) = json_number(json, "freq_m10") {
            self.config.freq_m10 = rounded_u32(freq_m10).clamp(1, 10000);
        }
        if let Some(freq_m11) = json_number(json, "freq_m11") {
            self.config.freq_m11 = rounded_u32(freq_m11).clamp(1, 10000);
        }
        if let Some(freq_m12) = json_number(json, "freq_m12") {
            self.config.freq_m12 = rounded_u32(freq_m12).clamp(1, 100);
        }
        if let Some(random) = json_bool(json, "random") {
            self.config.random = random;
        }
        if let Some(background) = json_string(json, "background") {
            if let Some(color) = parse_hex_color(background, rgb(0, 0, 0)) {
                self.config.background = color;
            }
        }
        if let Some(color) = json_string(json, "color") {
            if let Some(color) = parse_hex_color(color, rgb(255, 0, 0)) {
                self.config.color = color;
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = if self.width == 0 { pixels.len() } else { self.width.max(1) };
        let height = if self.height == 0 { 1 } else { self.height.max(1) };
        let cfg = self.config;

        let frequency = scaled(cfg.frequency);
        let amplitude = scaled(cfg.amplitude);
        let freq_m1 = scaled(cfg.freq_m1);
        let freq_m2 = scaled(cfg.freq_m2);
        let freq_m3 = scaled(cfg.freq_m3);
        let freq_m4 = scaled(cfg.freq_m4);
        let freq_m5 = scaled(cfg.freq_m5);
        let freq_m6 = scaled(cfg.freq_m6);
        let freq_m7 = scaled(cfg.freq_m7);
        let freq_m8 = scaled(cfg.freq_m8);
        let freq_m9 = scaled(cfg.freq_m9);
        let freq_m10 = scaled(cfg.freq_m10);
        let freq_m11 = scaled(cfg.freq_m11);
        let freq_m12 = scaled(cfg.freq_m12);

        let f = frequency * 0.01;
        let t_term = -0.01 * self.progress * cfg.speed as f32;
        let foreground = if cfg.random {
            lerp_rgb(self.current_random, self.next_random, self.random_tick.min(1.0))
        } else {
            cfg.color
        };

        let thickness = cfg.thickness as f32;
        let height_f = height as f32;
        let mut led = 0usize;
        for y in 0..height {
            let y = y as f32;
            for x in 0..width {
                if led >= pixels.len() {
                    self.advance_time(elapsed_seconds);
                    return;
                }

                let x = x as f32;
                let mut wave = (x * f).sin();
                wave += ((x * f * freq_m1) + t_term).sin() * freq_m2;
                wave += ((x * f * freq_m3) + (t_term * freq_m4)).sin() * freq_m5;
                wave += ((x * f * freq_m6) + (t_term * freq_m7)).sin() * freq_m8;
                wave += ((x * f * freq_m9) + (t_term * freq_m10)).sin() * freq_m11;
                wave *= 0.1 * amplitude * freq_m12;

                let curve_y = (1.0 + wave) * 0.5 * height_f;
                let distance = (curve_y - y).abs();
                pixels[led] = if distance > thickness {
                    cfg.background
                } else {
                    lerp_rgb(foreground, cfg.background, distance / thickness)
                };
                led += 1;
            }
        }

        if led < pixels.len() {
            for pixel in &mut pixels[led..] {
                *pixel = cfg.background;
            }
        }
        self.advance_time(elapsed_seconds);
    }

    fn advance_time(&mut self, elapsed_seconds: f64) {
        let mut dt = FRAME_DT;
        if let Some(last) = self.last_time {
            dt = (elapsed_seconds - last).max(0.0);
        }
        self.last_time = Some(elapsed_seconds);

        self.time_carry += dt;
        while self.time_carry >= FRAME_DT {
            self.step_reference_frame();
            self.time_carry -= FRAME_DT;
        }
    }

    fn step_reference_frame(&mut self) {
        let delta = self.config.speed as f32 * FRAME_DT as f32;

        if self.random_tick >= 1.0 {
            self.current_random = self.next_random;
            self.next_random = self.make_random_color();
            self.random_tick = 0.0;
        }

        self.random_tick += 0.005 * delta;
        self.progress += 0.1 * delta;
    }

    fn reset_runtime_state(&mut self) {
        self.progress = 0.0;
        self.random_tick = 0.0;
        self.last_time = None;
        self.time_carry = 0.0;
        self.current_random = self.make_random_color();
        self.next_random = self.make_random_color();
    }

    fn make_random_color(&mut self) -> SkydimoRgb {
        hsv_to_rgb(self.rng.next_unit() * 360.0, 1.0, 1.0)
    }
}

unsafe extern "C" fn fractal_motion_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(FractalMotionEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn fractal_motion_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<FractalMotionEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn fractal_motion_resize(
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

unsafe extern "C" fn fractal_motion_update_params_json(
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

unsafe extern "C" fn fractal_motion_tick(
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

unsafe extern "C" fn fractal_motion_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be writable for one host-compatible `SkydimoPluginApiV1`.
/// The host must request the ABI version declared in this plugin manifest.
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
                    create: Some(fractal_motion_create),
                    destroy: Some(fractal_motion_destroy),
                    resize: Some(fractal_motion_resize),
                    update_params_json: Some(fractal_motion_update_params_json),
                    tick: Some(fractal_motion_tick),
                    is_ready: Some(fractal_motion_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut FractalMotionEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<FractalMotionEffect>() })
    }
}

fn scaled(raw_value: u32) -> f32 {
    raw_value as f32 * VALUE_SCALE
}

fn lerp_rgb(left: SkydimoRgb, right: SkydimoRgb, t: f32) -> SkydimoRgb {
    let t = t.clamp(0.0, 1.0);
    SkydimoRgb {
        r: to_u8(left.r as f32 + (right.r as f32 - left.r as f32) * t),
        g: to_u8(left.g as f32 + (right.g as f32 - left.g as f32) * t),
        b: to_u8(left.b as f32 + (right.b as f32 - left.b as f32) * t),
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

    rgb(
        to_u8((r + m) * 255.0),
        to_u8((g + m) * 255.0),
        to_u8((b + m) * 255.0),
    )
}

fn parse_hex_color(raw: &str, fallback: SkydimoRgb) -> Option<SkydimoRgb> {
    let mut hex = raw.trim();
    if let Some(stripped) = hex.strip_prefix('#') {
        hex = stripped;
    }
    if hex.len() == 8 {
        hex = &hex[..6];
    }

    let bytes = hex.as_bytes();
    match bytes.len() {
        3 => Some(rgb(
            parse_hex_nibble(bytes[0])? * 17,
            parse_hex_nibble(bytes[1])? * 17,
            parse_hex_nibble(bytes[2])? * 17,
        )),
        6 => Some(rgb(
            parse_hex_byte(bytes[0], bytes[1])?,
            parse_hex_byte(bytes[2], bytes[3])?,
            parse_hex_byte(bytes[4], bytes[5])?,
        )),
        _ => Some(fallback),
    }
}

fn parse_hex_byte(high: u8, low: u8) -> Option<u8> {
    Some((parse_hex_nibble(high)? << 4) | parse_hex_nibble(low)?)
}

fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn json_number(json: &str, key: &str) -> Option<f32> {
    let value = json_value_after_key(json, key)?;
    let end = value
        .char_indices()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(value.len());
    value[..end].trim().parse::<f32>().ok()
}

fn json_bool(json: &str, key: &str) -> Option<bool> {
    let value = json_value_after_key(json, key)?;
    if value.starts_with("true") {
        Some(true)
    } else if value.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let value = json_value_after_key(json, key)?;
    let raw = value.strip_prefix('"')?;
    let end = json_string_end(raw)?;
    Some(&raw[..end])
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

fn rounded_u32(value: f32) -> u32 {
    (value + 0.5).floor().clamp(0.0, u32::MAX as f32) as u32
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

const fn rgb(r: u8, g: u8, b: u8) -> SkydimoRgb {
    SkydimoRgb { r, g, b }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn seed_from_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x6A09_E667_F3BC_C909)
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.max(1),
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x.max(1);
        x
    }

    #[inline]
    fn next_unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / ((1u64 << 24) - 1) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::{hsv_to_rgb, json_number, lerp_rgb, parse_hex_color, FractalMotionEffect};
    use crate::abi::SkydimoRgb;

    #[test]
    fn parses_params_without_json_dependency() {
        let mut effect = FractalMotionEffect::with_seed(1);
        effect.update_params(
            r##"{"speed":75,"thickness":4,"random":true,"background":"#112233","color":"#0af"}"##,
        );

        assert_eq!(effect.config.speed, 75);
        assert_eq!(effect.config.thickness, 4);
        assert!(effect.config.random);
        assert_eq!(effect.config.background, SkydimoRgb { r: 17, g: 34, b: 51 });
        assert_eq!(effect.config.color, SkydimoRgb { r: 0, g: 170, b: 255 });
        assert_eq!(json_number(r#"{"freq_m12":9}"#, "freq_m12"), Some(9.0));
    }

    #[test]
    fn color_helpers_match_expected_anchors() {
        assert_eq!(
            parse_hex_color("#FF000080", SkydimoRgb::default()),
            Some(SkydimoRgb { r: 255, g: 0, b: 0 })
        );
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(
            lerp_rgb(
                SkydimoRgb { r: 255, g: 0, b: 0 },
                SkydimoRgb { r: 0, g: 0, b: 0 },
                0.5,
            ),
            SkydimoRgb { r: 128, g: 0, b: 0 }
        );
    }

    #[test]
    fn renders_into_host_buffer() {
        let mut effect = FractalMotionEffect::with_seed(1);
        effect.resize(16, 4, 64);

        let mut pixels = [SkydimoRgb::default(); 64];
        effect.tick(0.0, &mut pixels);

        assert!(pixels
            .iter()
            .any(|pixel| pixel.r != 0 || pixel.g != 0 || pixel.b != 0));
    }
}
