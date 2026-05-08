mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

#[derive(Clone, Copy)]
struct Config {
    speed: f32,
    color_speed: f32,
    reverse: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 20.0,
            color_speed: 30.0,
            reverse: false,
        }
    }
}

struct RotatingRainbowEffect {
    config: Config,
    width: usize,
    height: usize,
}

impl RotatingRainbowEffect {
    fn new() -> Self {
        Self {
            config: Config::default(),
            width: 0,
            height: 0,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width as usize;
        self.height = height as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = parse_number_field(json, "speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(color_speed) = parse_number_field(json, "color_speed")
            .or_else(|| parse_number_field(json, "colorSpeed"))
        {
            self.config.color_speed = color_speed.clamp(1.0, 100.0);
        }
        if let Some(reverse) = parse_bool_field(json, "reverse") {
            self.config.reverse = reverse;
        }
    }

    fn tick(&self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        let n = pixels.len();
        if n == 0 {
            return;
        }

        let width = if self.width == 0 { n } else { self.width };
        let height = if self.height == 0 { 1 } else { self.height };
        let is_linear = height <= 1;

        let time = 1000.0 + 0.1 * self.config.speed * elapsed_seconds as f32;
        let rotation = if self.config.reverse { -time } else { time };
        let (sin_t, cos_t) = rotation.sin_cos();

        let cx = if is_linear {
            width as f32 * 0.5
        } else {
            (width.saturating_sub(1)) as f32 * 0.5
        };
        let cy = if is_linear {
            0.5
        } else {
            (height.saturating_sub(1)) as f32 * 0.5
        };

        let base_hue = time * self.config.color_speed;
        let hue_scale = 360.0 / 128.0;
        let hue_step = hue_scale * 2.0 * sin_t;
        let x_origin = -cx * 2.0 * sin_t;
        let mut idx = 0usize;

        for y in 0..height {
            let fy = if is_linear { 0.5 } else { y as f32 };
            let dy_cos = (fy - cy) * 2.0 * cos_t;
            let mut hue = normalize_hue(base_hue + hue_scale * (dy_cos + x_origin));

            for _ in 0..width {
                if idx >= n {
                    return;
                }
                pixels[idx] = hue_to_rgb(hue);
                hue = advance_hue(hue, hue_step);
                idx += 1;
            }
        }
    }
}

unsafe extern "C" fn rotating_rainbow_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(RotatingRainbowEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn rotating_rainbow_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<RotatingRainbowEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn rotating_rainbow_resize(
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

unsafe extern "C" fn rotating_rainbow_update_params_json(
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

unsafe extern "C" fn rotating_rainbow_tick(
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

unsafe extern "C" fn rotating_rainbow_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer for one `SkydimoPluginApiV1`.
/// The host must request the ABI version declared by this plugin manifest.
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
                    create: Some(rotating_rainbow_create),
                    destroy: Some(rotating_rainbow_destroy),
                    resize: Some(rotating_rainbow_resize),
                    update_params_json: Some(rotating_rainbow_update_params_json),
                    tick: Some(rotating_rainbow_tick),
                    is_ready: Some(rotating_rainbow_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut RotatingRainbowEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<RotatingRainbowEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline(always)]
fn normalize_hue(hue: f32) -> f32 {
    hue.rem_euclid(360.0)
}

#[inline(always)]
fn advance_hue(hue: f32, step: f32) -> f32 {
    let mut next = hue + step;
    if next >= 360.0 {
        next -= 360.0;
    } else if next < 0.0 {
        next += 360.0;
    }
    next
}

#[inline(always)]
fn hue_to_rgb(hue: f32) -> SkydimoRgb {
    let scaled = hue * (1.0 / 60.0);
    let sector = scaled as u32;
    let f = scaled - sector as f32;
    let up = to_u8_unit(f);
    let down = to_u8_unit(1.0 - f);

    match sector {
        0 => SkydimoRgb {
            r: 255,
            g: up,
            b: 0,
        },
        1 => SkydimoRgb {
            r: down,
            g: 255,
            b: 0,
        },
        2 => SkydimoRgb {
            r: 0,
            g: 255,
            b: up,
        },
        3 => SkydimoRgb {
            r: 0,
            g: down,
            b: 255,
        },
        4 => SkydimoRgb {
            r: up,
            g: 0,
            b: 255,
        },
        _ => SkydimoRgb {
            r: 255,
            g: 0,
            b: down,
        },
    }
}

#[inline(always)]
fn to_u8_unit(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

fn parse_number_field(json: &str, key: &str) -> Option<f32> {
    let raw = json_value_slice(json, key)?;
    raw.parse::<f32>().ok()
}

fn parse_bool_field(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_slice(json, key)?;
    match raw {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::{hue_to_rgb, RotatingRainbowEffect};
    use crate::abi::SkydimoRgb;

    #[test]
    fn parses_params_without_json_dependency() {
        let mut effect = RotatingRainbowEffect::new();
        effect.update_params(r#"{"speed":42,"color_speed":17,"reverse":true}"#);
        assert_eq!(effect.config.speed, 42.0);
        assert_eq!(effect.config.color_speed, 17.0);
        assert!(effect.config.reverse);
    }

    #[test]
    fn hue_conversion_matches_full_saturation_anchors() {
        let red = hue_to_rgb(0.0);
        let green = hue_to_rgb(120.0);
        let blue = hue_to_rgb(240.0);

        assert_eq!((red.r, red.g, red.b), (255, 0, 0));
        assert_eq!((green.r, green.g, green.b), (0, 255, 0));
        assert_eq!((blue.r, blue.g, blue.b), (0, 0, 255));
    }

    #[test]
    fn renders_directly_into_host_buffer() {
        let mut effect = RotatingRainbowEffect::new();
        effect.resize(8, 1);

        let mut pixels = [SkydimoRgb::default(); 8];
        effect.tick(0.25, &mut pixels);

        assert!(pixels.iter().any(|pixel| pixel.r != 0 || pixel.g != 0 || pixel.b != 0));
    }
}
