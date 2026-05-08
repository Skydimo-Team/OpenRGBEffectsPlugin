mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const PI: f32 = std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
struct HsvColor {
    h: f32,
    s: f32,
}

#[derive(Clone)]
struct Config {
    speed: f32,
    random_enabled: bool,
    palette: Vec<HsvColor>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 50.0,
            random_enabled: false,
            palette: vec![
                HsvColor { h: 0.0, s: 1.0 },
                HsvColor { h: 200.0, s: 1.0 },
                HsvColor { h: 24.0, s: 1.0 },
            ],
        }
    }
}

struct BreathingEffect {
    config: Config,
    rng: XorShift64,
    last_cycle: i64,
    random_hue: f32,
    parsed_palette: Vec<HsvColor>,
}

impl BreathingEffect {
    fn new() -> Self {
        Self {
            config: Config::default(),
            rng: XorShift64::seeded(),
            last_cycle: -1,
            random_hue: 0.0,
            parsed_palette: Vec::new(),
        }
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(random_enabled) = json_bool(json, "random") {
            self.config.random_enabled = random_enabled;
        }
        if collect_hsv_palette(json, "colors", &mut self.parsed_palette)
            && !self.parsed_palette.is_empty()
        {
            std::mem::swap(&mut self.config.palette, &mut self.parsed_palette);
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let progress = (elapsed_seconds.max(0.0) as f32) * (self.config.speed * 0.02);
        let cycle = (progress / PI).floor() as i64;
        let phase = progress - cycle as f32 * PI;
        let sin_value = phase.sin();
        let value = sin_value * sin_value * sin_value;

        let hsv = if self.config.random_enabled {
            if cycle != self.last_cycle {
                self.last_cycle = cycle;
                self.random_hue = self.rng.next_f32() * 360.0;
            }
            HsvColor {
                h: self.random_hue,
                s: 1.0,
            }
        } else if self.config.palette.is_empty() {
            fill_rgb(pixels, SkydimoRgb::default());
            return;
        } else {
            let index = cycle.rem_euclid(self.config.palette.len() as i64) as usize;
            self.config.palette[index]
        };

        fill_rgb(pixels, hsv_to_rgb(hsv.h, hsv.s, value));
    }
}

unsafe extern "C" fn breathing_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    ffi_status(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host = unsafe { &*host };
            if host.abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(BreathingEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn breathing_destroy(instance: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<BreathingEffect>()));
            }
        }
    }));
}

unsafe extern "C" fn breathing_resize(
    instance: *mut c_void,
    _width: u32,
    _height: u32,
    _led_count: u32,
) -> i32 {
    ffi_status(|| {
        if instance.is_null() {
            -1
        } else {
            0
        }
    })
}

unsafe extern "C" fn breathing_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
    ffi_status(|| {
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

unsafe extern "C" fn breathing_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    ffi_status(|| {
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

unsafe extern "C" fn breathing_is_ready(instance: *mut c_void) -> i32 {
    if instance.is_null() {
        -1
    } else {
        1
    }
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to `SkydimoPluginApiV1`.
/// The host must request the ABI version declared in this plugin's manifest.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    ffi_status(|| {
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
                    create: Some(breathing_create),
                    destroy: Some(breathing_destroy),
                    resize: Some(breathing_resize),
                    update_params_json: Some(breathing_update_params_json),
                    tick: Some(breathing_tick),
                    is_ready: Some(breathing_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut BreathingEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<BreathingEffect>() })
    }
}

fn ffi_status(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn collect_hsv_palette(json: &str, key: &str, out: &mut Vec<HsvColor>) -> bool {
    let Some(raw) = json_value_start(json, key) else {
        return false;
    };
    let bytes = raw.as_bytes();
    if bytes.first().copied() != Some(b'[') {
        return false;
    }

    out.clear();
    let mut i = 1usize;
    let mut scratch = String::new();
    while i < bytes.len() {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\n' | b'\r' | b'\t' | b',') {
            i += 1;
        }
        if i >= bytes.len() {
            return false;
        }
        if bytes[i] == b']' {
            return true;
        }
        if bytes[i] != b'"' {
            i = skip_json_value(bytes, i);
            continue;
        }

        let start = i + 1;
        i = start;
        let mut escaped = false;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    escaped = true;
                    i = i.saturating_add(2);
                }
                b'"' => break,
                _ => i += 1,
            }
        }
        if i >= bytes.len() {
            return false;
        }

        if escaped {
            scratch.clear();
            unescape_json_string(&raw[start..i], &mut scratch);
            if let Some(hsv) = parse_hex_to_hsv(scratch.as_str()) {
                out.push(hsv);
            }
        } else if let Some(hsv) = parse_hex_to_hsv(&raw[start..i]) {
            out.push(hsv);
        }
        i += 1;
    }
    false
}

fn skip_json_value(bytes: &[u8], mut i: usize) -> usize {
    let mut in_string = false;
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => i = i.saturating_add(2),
            b'"' => {
                in_string = !in_string;
                i += 1;
            }
            b'[' | b'{' if !in_string => {
                depth += 1;
                i += 1;
            }
            b']' | b'}' if !in_string => {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
                i += 1;
            }
            b',' if !in_string && depth == 0 => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn unescape_json_string(raw: &str, out: &mut String) {
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 1;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
}

fn json_number(json: &str, key: &str) -> Option<f32> {
    let raw = json_value_start(json, key)?;
    let value = if let Some(rest) = raw.strip_prefix('"') {
        let end = rest.find('"')?;
        &rest[..end]
    } else {
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
        &raw[..end]
    };
    value.trim().parse::<f32>().ok()
}

fn json_bool(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_start(json, key)?;
    if raw.starts_with("true") {
        Some(true)
    } else if raw.starts_with("false") {
        Some(false)
    } else {
        json_number(json, key).map(|value| value != 0.0)
    }
}

fn json_value_start<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(needle.as_str())?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

fn parse_hex_to_hsv(raw: &str) -> Option<HsvColor> {
    let rgb = parse_hex_color(raw)?;
    let (h, s, _) = rgb_to_hsv(rgb);
    Some(HsvColor { h, s })
}

fn parse_hex_color(raw: &str) -> Option<SkydimoRgb> {
    let mut hex = [0u8; 6];
    let mut len = 0usize;
    for byte in raw.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'#' && len == 0 {
            continue;
        }
        if len == hex.len() {
            return None;
        }
        hex[len] = byte;
        len += 1;
    }

    match len {
        3 => Some(SkydimoRgb {
            r: parse_hex_nibble(hex[0])? * 17,
            g: parse_hex_nibble(hex[1])? * 17,
            b: parse_hex_nibble(hex[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: parse_hex_byte(hex[0], hex[1])?,
            g: parse_hex_byte(hex[2], hex[3])?,
            b: parse_hex_byte(hex[4], hex[5])?,
        }),
        _ => None,
    }
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

fn rgb_to_hsv(rgb: SkydimoRgb) -> (f32, f32, f32) {
    let rf = rgb.r as f32 / 255.0;
    let gf = rgb.g as f32 / 255.0;
    let bf = rgb.b as f32 / 255.0;
    let maxc = rf.max(gf).max(bf);
    let minc = rf.min(gf).min(bf);
    let delta = maxc - minc;

    let hue = if delta == 0.0 {
        0.0
    } else if maxc == rf {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if maxc == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let saturation = if maxc == 0.0 { 0.0 } else { delta / maxc };
    (hue.rem_euclid(360.0), saturation, maxc)
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

#[inline]
fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn fill_rgb(buffer: &mut [SkydimoRgb], color: SkydimoRgb) {
    if buffer.is_empty() {
        return;
    }

    buffer[0] = color;
    let mut filled = 1usize;
    while filled < buffer.len() {
        let copy_len = filled.min(buffer.len() - filled);
        unsafe {
            std::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                buffer.as_mut_ptr().add(filled),
                copy_len,
            );
        }
        filled += copy_len;
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
        Self::new(nanos ^ 0xA076_1D64_78BD_642F)
    }

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
    fn next_f32(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 16_777_216.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_hsv_palette, fill_rgb, hsv_to_rgb, parse_hex_to_hsv, skydimo_plugin_get_api,
        BreathingEffect, HsvColor, SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION,
        SKYDIMO_PLUGIN_KIND_EFFECT, PI,
    };

    #[test]
    fn parses_palette_to_hsv_without_keeping_source_brightness() {
        assert_eq!(parse_hex_to_hsv("#FF0000"), Some(HsvColor { h: 0.0, s: 1.0 }));
        assert_eq!(parse_hex_to_hsv(" # F00 "), Some(HsvColor { h: 0.0, s: 1.0 }));
        let sky = parse_hex_to_hsv("#00AAFF").expect("sky color should parse");
        assert!((sky.h - 200.0).abs() < 0.01);
        assert!((sky.s - 1.0).abs() < 0.01);
    }

    #[test]
    fn updates_params_from_manifest_shape() {
        let mut effect = BreathingEffect::new();
        effect.update_params(r##"{"speed":75,"random":true,"colors":["#000","#0af"]}"##);

        assert_eq!(effect.config.speed, 75.0);
        assert!(effect.config.random_enabled);
        assert_eq!(effect.config.palette.len(), 2);
        assert_eq!(effect.config.palette[0], HsvColor { h: 0.0, s: 0.0 });
    }

    #[test]
    fn renders_default_colors_per_breathing_cycle() {
        let mut effect = BreathingEffect::new();
        let mut pixels = [SkydimoRgb::default(); 8];

        effect.tick((PI * 0.5) as f64, &mut pixels);
        assert!(pixels.iter().all(|pixel| *pixel == SkydimoRgb { r: 255, g: 0, b: 0 }));

        effect.tick((PI + PI * 0.5) as f64, &mut pixels);
        assert!(pixels
            .iter()
            .all(|pixel| *pixel == SkydimoRgb { r: 0, g: 170, b: 255 }));
    }

    #[test]
    fn ignores_empty_palette_updates() {
        let mut effect = BreathingEffect::new();
        effect.update_params(r##"{"colors":[]}"##);
        assert_eq!(effect.config.palette.len(), 3);
    }

    #[test]
    fn collects_palette_and_skips_invalid_entries() {
        let mut palette = Vec::new();
        assert!(collect_hsv_palette(
            r##"{"colors":["#ff0000",42,"bad-color","#00AAFF"]}"##,
            "colors",
            &mut palette,
        ));
        assert_eq!(palette.len(), 2);
    }

    #[test]
    fn fills_whole_buffer_by_copy_doubling() {
        let color = SkydimoRgb { r: 7, g: 8, b: 9 };
        let mut pixels = [SkydimoRgb::default(); 31];
        fill_rgb(&mut pixels, color);
        assert!(pixels.iter().all(|pixel| *pixel == color));
    }

    #[test]
    fn exported_api_declares_effect_v3() {
        let mut api = SkydimoPluginApiV1::default();
        let status = unsafe {
            skydimo_plugin_get_api(SKYDIMO_NATIVE_C_ABI_VERSION, std::ptr::null(), &mut api)
        };

        assert_eq!(status, 0);
        assert_eq!(api.abi_version, SKYDIMO_NATIVE_C_ABI_VERSION);
        assert_eq!(api.kind_mask & SKYDIMO_PLUGIN_KIND_EFFECT, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }

    #[test]
    fn hsv_rounds_like_lua_buffer_conversion() {
        assert_eq!(hsv_to_rgb(24.0, 1.0, 1.0), SkydimoRgb { r: 255, g: 102, b: 0 });
    }
}
