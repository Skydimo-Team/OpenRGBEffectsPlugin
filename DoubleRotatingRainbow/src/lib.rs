mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const HUE_SCALE: f32 = 360.0 / 128.0;
const INV_SIXTY: f32 = 1.0 / 60.0;
const INV_SIX: f32 = 1.0 / 6.0;

#[derive(Clone, Copy)]
struct Config {
    speed: f32,
    color_speed: f32,
    frequency: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 50.0,
            color_speed: 20.0,
            frequency: 1.0,
        }
    }
}

struct DoubleRotatingRainbowEffect {
    config: Config,
    width: usize,
    height: usize,
    cached_width: usize,
    x_offsets: Vec<f32>,
}

impl Default for DoubleRotatingRainbowEffect {
    fn default() -> Self {
        Self {
            config: Config::default(),
            width: 0,
            height: 1,
            cached_width: 0,
            x_offsets: Vec::new(),
        }
    }
}

impl DoubleRotatingRainbowEffect {
    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = if width == 0 {
            led_count as usize
        } else {
            width as usize
        };
        self.height = height.max(1) as usize;
        self.rebuild_x_offsets();
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
        if let Some(frequency) = parse_number_field(json, "frequency") {
            self.config.frequency = frequency.clamp(1.0, 20.0);
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
        if self.cached_width != width || self.x_offsets.len() != width {
            self.width = width;
            self.rebuild_x_offsets();
        }

        let time = elapsed_seconds as f32 * 0.01 * self.config.speed;
        let (sin_t, cos_t) = time.sin_cos();
        let cy = if height > 1 {
            height.saturating_sub(1) as f32 * 0.5
        } else {
            0.5
        };

        let base_hue = time * self.config.color_speed;
        let freq = self.config.frequency;
        let row_factor = freq * cos_t;
        let x_factor = freq * sin_t;
        let mut index = 0usize;

        for y in 0..height {
            let row_hue = base_hue + HUE_SCALE * ((y as f32 - cy) * row_factor);
            for x_offset in &self.x_offsets {
                if index >= pixels.len() {
                    return;
                }

                let hue = row_hue + HUE_SCALE * (*x_offset * x_factor);
                pixels[index] = hue_to_rgb(hue);
                index += 1;
            }
        }

        if index < pixels.len() {
            pixels[index..].fill(SkydimoRgb::default());
        }
    }

    fn rebuild_x_offsets(&mut self) {
        let width = self.width;
        self.cached_width = width;
        self.x_offsets.clear();
        self.x_offsets.reserve(width);
        if width == 0 {
            return;
        }

        let center_x = width.saturating_sub(1) as f32 * 0.5;
        let half_width = width as f32 * 0.44;
        for x in 0..width {
            self.x_offsets
                .push(half_width - (x as f32 - center_x).abs());
        }
    }
}

unsafe extern "C" fn double_rotating_rainbow_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(DoubleRotatingRainbowEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn double_rotating_rainbow_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<DoubleRotatingRainbowEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn double_rotating_rainbow_resize(
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

unsafe extern "C" fn double_rotating_rainbow_update_params_json(
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

unsafe extern "C" fn double_rotating_rainbow_tick(
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

unsafe extern "C" fn double_rotating_rainbow_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer to a host-compatible
/// `SkydimoPluginApiV1`. The host must pass the ABI version declared in the
/// plugin manifest.
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
                    create: Some(double_rotating_rainbow_create),
                    destroy: Some(double_rotating_rainbow_destroy),
                    resize: Some(double_rotating_rainbow_resize),
                    update_params_json: Some(double_rotating_rainbow_update_params_json),
                    tick: Some(double_rotating_rainbow_tick),
                    is_ready: Some(double_rotating_rainbow_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut DoubleRotatingRainbowEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<DoubleRotatingRainbowEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline(always)]
fn hue_to_rgb(hue: f32) -> SkydimoRgb {
    if !hue.is_finite() {
        return SkydimoRgb::default();
    }

    let scaled = hue * INV_SIXTY;
    let wrapped = scaled - 6.0 * (scaled * INV_SIX).floor();
    let sector = wrapped as u32;
    let f = wrapped - sector as f32;
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
    (value * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn parse_number_field(json: &str, key: &str) -> Option<f32> {
    let raw = json_value_slice(json, key)?;
    let value = raw.parse::<f32>().ok()?;
    value.is_finite().then_some(value)
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
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
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
    use super::{hue_to_rgb, DoubleRotatingRainbowEffect};
    use crate::abi::SkydimoRgb;

    #[test]
    fn parses_params_without_json_dependency() {
        let mut effect = DoubleRotatingRainbowEffect::default();
        effect.update_params(r#"{"speed":75,"color_speed":40,"frequency":7}"#);
        assert_eq!(effect.config.speed, 75.0);
        assert_eq!(effect.config.color_speed, 40.0);
        assert_eq!(effect.config.frequency, 7.0);
    }

    #[test]
    fn hue_conversion_matches_primary_anchors() {
        assert_eq!(hue_to_rgb(0.0), SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(hue_to_rgb(120.0), SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(hue_to_rgb(240.0), SkydimoRgb { r: 0, g: 0, b: 255 });
        assert_eq!(hue_to_rgb(-120.0), SkydimoRgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn renders_directly_into_host_buffer() {
        let mut effect = DoubleRotatingRainbowEffect::default();
        effect.resize(8, 1, 8);

        let mut pixels = [SkydimoRgb::default(); 8];
        effect.tick(0.25, &mut pixels);

        assert!(pixels.iter().any(|pixel| *pixel != SkydimoRgb::default()));
        assert_eq!(effect.x_offsets.len(), 8);
    }
}
