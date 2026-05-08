mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const GRADIENT_SAMPLES: usize = 100;
const EPSILON: f32 = 1.0e-9;

#[derive(Clone, Copy, Default)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Clone, Copy)]
struct Config {
    speed: f32,
    max_intensity: f32,
    intensity_speed: f32,
    radius: f32,
    grow_speed: f32,
    motion: bool,
    run_once: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 10.0,
            max_intensity: 80.0,
            intensity_speed: 10.0,
            radius: 50.0,
            grow_speed: 10.0,
            motion: false,
            run_once: false,
        }
    }
}

struct SunriseEffect {
    config: Config,
    user_colors: [Rgb; 4],
    gradient: [SkydimoRgb; GRADIENT_SAMPLES],
    width: usize,
    height: usize,
}

impl SunriseEffect {
    fn new() -> Self {
        Self {
            config: Config::default(),
            user_colors: [
                Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                Rgb {
                    r: 255,
                    g: 255,
                    b: 0,
                },
                Rgb { r: 255, g: 0, b: 0 },
                Rgb { r: 0, g: 0, b: 0 },
            ],
            gradient: [SkydimoRgb::default(); GRADIENT_SAMPLES],
            width: 0,
            height: 1,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1) as usize;
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(value) = parse_number_field(json, "speed") {
            self.config.speed = round_clamp(value, 1.0, 20.0);
        }
        if let Some(value) = parse_number_field(json, "intensity") {
            self.config.max_intensity = round_clamp(value, 1.0, 99.0);
        }
        if let Some(value) = parse_number_field(json, "intensity_speed") {
            self.config.intensity_speed = round_clamp(value, 1.0, 100.0);
        }
        if let Some(value) = parse_number_field(json, "radius") {
            self.config.radius = round_clamp(value, 1.0, 100.0);
        }
        if let Some(value) = parse_number_field(json, "grow_speed") {
            self.config.grow_speed = round_clamp(value, 1.0, 50.0);
        }
        if let Some(value) = parse_bool_field(json, "run_once") {
            self.config.run_once = value;
        }
        if let Some(value) = parse_bool_field(json, "motion") {
            self.config.motion = value;
        }
        parse_color_array_field(json, "colors", &mut self.user_colors);
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
        let total = pixels.len().min(width.saturating_mul(height));
        if total == 0 {
            return;
        }

        let time_val = 0.1 * self.config.speed * elapsed_seconds as f32;
        let (progress, y_shift) = if self.config.run_once {
            (time_val.min(1.0), 0.0)
        } else {
            let progress = 0.5 * (1.0 + time_val.sin());
            (progress, -1.0 + 2.0 * progress)
        };

        let first_stop = (0.01 * self.config.max_intensity)
            .min(progress.powf(0.1 * self.config.intensity_speed));
        let second_stop = first_stop + (1.0 - first_stop) * 0.5;
        self.rebuild_gradient(first_stop, second_stop);

        let width_f = width as f32;
        let height_f = height as f32;
        let real_radius =
            0.01 * self.config.radius * width_f * progress.powf(0.1 * self.config.grow_speed);
        let inv_radius = if real_radius > EPSILON {
            1.0 / real_radius
        } else {
            0.0
        };
        let hx = 0.5 * (width_f - 1.0);
        let hy = 0.5 * (height_f - 1.0);
        let motion_shift = if self.config.motion { hy * y_shift } else { 0.0 };

        let mut index = 0usize;
        for y in 0..height {
            if index >= total {
                break;
            }
            let dy = y as f32 + motion_shift - hy;
            let dy2 = dy * dy;

            for x in 0..width {
                if index >= total {
                    break;
                }

                let dx = x as f32 - hx;
                let distance = dx.mul_add(dx, dy2).sqrt();
                let percent = if real_radius > EPSILON {
                    (distance * inv_radius).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let sample = ((GRADIENT_SAMPLES - 1) as f32 * percent)
                    .floor()
                    .clamp(0.0, (GRADIENT_SAMPLES - 1) as f32) as usize;
                pixels[index] = self.gradient[sample];
                index += 1;
            }
        }

        if index < pixels.len() {
            pixels[index..].fill(SkydimoRgb::default());
        }
    }

    fn rebuild_gradient(&mut self, first_stop: f32, second_stop: f32) {
        let s1 = first_stop.clamp(0.0, 1.0);
        let s2 = second_stop.clamp(s1, 1.0);
        let [c0, c1, c2, c3] = self.user_colors;

        for (sample, out) in self.gradient.iter_mut().enumerate() {
            let t = (sample as f32 + 0.5) / GRADIENT_SAMPLES as f32;
            let (left, right, blend) = if t < s1 {
                (c0, c1, t / s1.max(EPSILON))
            } else if t < s2 {
                (c1, c2, (t - s1) / (s2 - s1).max(EPSILON))
            } else {
                (c2, c3, (t - s2) / (1.0 - s2).max(EPSILON))
            };

            *out = SkydimoRgb {
                r: lerp_channel(left.r, right.r, blend),
                g: lerp_channel(left.g, right.g, blend),
                b: lerp_channel(left.b, right.b, blend),
            };
        }
    }
}

unsafe extern "C" fn sunrise_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host_api = unsafe { &*host };
            if host_api.abi_version < SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(SunriseEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn sunrise_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<SunriseEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn sunrise_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    _led_count: u32,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        effect.resize(width, height);
        0
    })
}

unsafe extern "C" fn sunrise_update_params_json(
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

unsafe extern "C" fn sunrise_tick(
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

unsafe extern "C" fn sunrise_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer. The host must pass the ABI
/// version declared in the plugin manifest.
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
                    create: Some(sunrise_create),
                    destroy: Some(sunrise_destroy),
                    resize: Some(sunrise_resize),
                    update_params_json: Some(sunrise_update_params_json),
                    tick: Some(sunrise_tick),
                    is_ready: Some(sunrise_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut SunriseEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<SunriseEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn round_clamp(value: f32, min: f32, max: f32) -> f32 {
    (value + 0.5).floor().clamp(min, max)
}

fn lerp_channel(a: u8, b: u8, t: f32) -> u8 {
    let a = a as f32;
    let b = b as f32;
    (a + (b - a) * t + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn parse_number_field(json: &str, key: &str) -> Option<f32> {
    json_value_slice(json, key)?.parse::<f32>().ok()
}

fn parse_bool_field(json: &str, key: &str) -> Option<bool> {
    match json_value_slice(json, key)? {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_color_array_field(json: &str, key: &str, colors: &mut [Rgb; 4]) {
    let Some(raw) = json_array_slice(json, key) else {
        return;
    };

    let bytes = raw.as_bytes();
    let mut i = 0usize;
    let mut slot = 0usize;
    while i < bytes.len() && slot < colors.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }

        let start = i + 1;
        i = start;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i > start {
            if let Some(color) = parse_hex_color(&raw[start..i]) {
                colors[slot] = color;
            }
            slot += 1;
        }
        i = i.saturating_add(1);
    }
}

fn json_value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    let raw = after_key[colon + 1..].trim_start();

    if let Some(rest) = raw.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].trim());
    }

    let end = raw
        .char_indices()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '+' | '.') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(raw.len());
    Some(raw[..end].trim())
}

fn json_array_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    let raw = after_key[colon + 1..].trim_start();
    let rest = raw.strip_prefix('[')?;
    let end = rest.find(']')?;
    Some(&rest[..end])
}

fn parse_hex_color(raw: &str) -> Option<Rgb> {
    let hex = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
    let bytes = hex.as_bytes();

    match bytes.len() {
        3 => Some(Rgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        }),
        6 => Some(Rgb {
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

#[cfg(test)]
mod tests {
    use super::{
        parse_bool_field, parse_color_array_field, parse_number_field, skydimo_plugin_get_api, Rgb,
        SunriseEffect,
    };
    use crate::abi::{
        SkydimoPluginApiV1, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_sunrise_params_without_allocating_json_values() {
        let json = r##"{
            "speed": 11,
            "run_once": true,
            "motion": false,
            "colors": ["#123456", "#abc", "#FF0000", "#000000"]
        }"##;
        assert_eq!(parse_number_field(json, "speed"), Some(11.0));
        assert_eq!(parse_bool_field(json, "run_once"), Some(true));
        assert_eq!(parse_bool_field(json, "motion"), Some(false));

        let mut colors = [Rgb::default(); 4];
        parse_color_array_field(json, "colors", &mut colors);
        assert_eq!((colors[0].r, colors[0].g, colors[0].b), (0x12, 0x34, 0x56));
        assert_eq!((colors[1].r, colors[1].g, colors[1].b), (0xaa, 0xbb, 0xcc));
    }

    #[test]
    fn renders_expected_buffer_length() {
        let mut effect = SunriseEffect::new();
        effect.resize(4, 3);
        let mut pixels = [super::SkydimoRgb::default(); 12];
        effect.tick(0.25, &mut pixels);
        assert_eq!(pixels.len(), 12);
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
        assert_eq!(api.kind_mask & SKYDIMO_PLUGIN_KIND_EFFECT, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }
}
