mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const CUSTOM_PRESET: u32 = 0;
const RAINBOW_PRESET: u32 = 1;
const SUNSET_PRESET: u32 = 2;
const OCEAN_PRESET: u32 = 3;
const SYNTHWAVE_PRESET: u32 = 4;

const DEFAULT_COLORS: [SkydimoRgb; 6] = [
    rgb(255, 0, 0),
    rgb(255, 153, 0),
    rgb(255, 255, 0),
    rgb(0, 255, 136),
    rgb(0, 170, 255),
    rgb(170, 0, 255),
];
const SUNSET_COLORS: [SkydimoRgb; 4] = [
    rgb(255, 94, 77),
    rgb(255, 154, 0),
    rgb(255, 206, 84),
    rgb(255, 111, 145),
];
const OCEAN_COLORS: [SkydimoRgb; 4] = [
    rgb(0, 88, 255),
    rgb(0, 170, 255),
    rgb(0, 255, 204),
    rgb(126, 255, 245),
];
const SYNTHWAVE_COLORS: [SkydimoRgb; 4] = [
    rgb(255, 0, 128),
    rgb(255, 71, 195),
    rgb(125, 65, 255),
    rgb(0, 217, 255),
];

struct RainbowEffect {
    speed: f32,
    preset: u32,
    custom_colors: Vec<SkydimoRgb>,
    colors_scratch: Vec<SkydimoRgb>,
    width: usize,
    height: usize,
}

impl RainbowEffect {
    fn new() -> Self {
        Self {
            speed: 2.5,
            preset: CUSTOM_PRESET,
            custom_colors: DEFAULT_COLORS.to_vec(),
            colors_scratch: Vec::with_capacity(16),
            width: 0,
            height: 0,
        }
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
        if let Some(speed) = parse_number_field(json, "speed") {
            self.speed = speed.clamp(0.0, 5.0);
        }
        if let Some(preset) = parse_number_field(json, "preset") {
            let preset = (preset + 0.5).floor() as u32;
            self.preset = if matches!(
                preset,
                CUSTOM_PRESET | RAINBOW_PRESET | SUNSET_PRESET | OCEAN_PRESET | SYNTHWAVE_PRESET
            ) {
                preset
            } else {
                CUSTOM_PRESET
            };
        }
        if parse_color_array_field(json, "colors", &mut self.colors_scratch) {
            if self.colors_scratch.is_empty() {
                self.custom_colors.clear();
                self.custom_colors.extend_from_slice(&DEFAULT_COLORS);
            } else {
                self.custom_colors.clear();
                self.custom_colors.extend_from_slice(&self.colors_scratch);
            }
        }
    }

    fn tick(&self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = if self.width == 0 { pixels.len() } else { self.width.max(1) };
        let height = if self.height == 0 { 1 } else { self.height.max(1) };
        let palette = self.active_palette();
        let offset = ((elapsed_seconds as f32) * self.speed * 0.12).rem_euclid(1.0);
        let row_shift = if height > 1 { 0.16 / height as f32 } else { 0.0 };

        let mut index = 0usize;
        for y in 0..height {
            let y_shift = y as f32 * row_shift;
            for x in 0..width {
                if index >= pixels.len() {
                    return;
                }
                let position = offset + (x as f32 / width as f32) + y_shift;
                pixels[index] = sample_gradient(palette, position);
                index += 1;
            }
        }

        if index < pixels.len() {
            for pixel in &mut pixels[index..] {
                *pixel = SkydimoRgb::default();
            }
        }
    }

    fn active_palette(&self) -> &[SkydimoRgb] {
        match self.preset {
            RAINBOW_PRESET => &DEFAULT_COLORS,
            SUNSET_PRESET => &SUNSET_COLORS,
            OCEAN_PRESET => &OCEAN_COLORS,
            SYNTHWAVE_PRESET => &SYNTHWAVE_COLORS,
            _ => self.custom_colors.as_slice(),
        }
    }
}

unsafe extern "C" fn rainbow_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(RainbowEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn rainbow_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<RainbowEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn rainbow_resize(
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

unsafe extern "C" fn rainbow_update_params_json(
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

unsafe extern "C" fn rainbow_tick(
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

unsafe extern "C" fn rainbow_is_ready(instance: *mut c_void) -> i32 {
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
                    create: Some(rainbow_create),
                    destroy: Some(rainbow_destroy),
                    resize: Some(rainbow_resize),
                    update_params_json: Some(rainbow_update_params_json),
                    tick: Some(rainbow_tick),
                    is_ready: Some(rainbow_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut RainbowEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<RainbowEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline(always)]
const fn rgb(r: u8, g: u8, b: u8) -> SkydimoRgb {
    SkydimoRgb { r, g, b }
}

#[inline(always)]
fn lerp_u8(left: u8, right: u8, t: f32) -> u8 {
    (left as f32 + (right as f32 - left as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn sample_gradient(palette: &[SkydimoRgb], position: f32) -> SkydimoRgb {
    match palette.len() {
        0 => rgb(255, 255, 255),
        1 => palette[0],
        count => {
            let wrapped = position.rem_euclid(1.0);
            let scaled = wrapped * count as f32;
            let left_index = scaled.floor() as usize % count;
            let right_index = (left_index + 1) % count;
            let blend = scaled - scaled.floor();
            let left = palette[left_index];
            let right = palette[right_index];
            rgb(
                lerp_u8(left.r, right.r, blend),
                lerp_u8(left.g, right.g, blend),
                lerp_u8(left.b, right.b, blend),
            )
        }
    }
}

fn parse_number_field(json: &str, key: &str) -> Option<f32> {
    json_value_slice(json, key)?.parse::<f32>().ok()
}

fn parse_color_array_field(json: &str, key: &str, out: &mut Vec<SkydimoRgb>) -> bool {
    let Some(raw) = json_value_slice(json, key) else {
        return false;
    };
    let bytes = raw.as_bytes();
    if bytes.first().copied() != Some(b'[') {
        return false;
    }

    out.clear();
    let mut index = 1usize;
    while index < bytes.len() {
        while index < bytes.len()
            && matches!(bytes[index], b' ' | b'\n' | b'\r' | b'\t' | b',')
        {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] == b']' {
            break;
        }
        if bytes[index] != b'"' {
            return false;
        }
        index += 1;

        let start = index;
        while index < bytes.len() && bytes[index] != b'"' {
            if bytes[index] == b'\\' {
                return false;
            }
            index += 1;
        }
        if index >= bytes.len() {
            return false;
        }
        if let Ok(raw) = std::str::from_utf8(&bytes[start..index]) {
            if let Some(color) = parse_hex_color(raw) {
                out.push(color);
            }
        }
        index += 1;
    }

    true
}

fn json_value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    let raw = after_key[colon + 1..].trim_start();

    if raw.starts_with('[') {
        let end = bracket_end(raw)?;
        return Some(raw[..end].trim());
    }
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

fn bracket_end(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if in_string => index += 2,
            b'"' => {
                in_string = !in_string;
                index += 1;
            }
            b'[' if !in_string => {
                depth += 1;
                index += 1;
            }
            b']' if !in_string => {
                depth = depth.saturating_sub(1);
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn parse_hex_color(raw: &str) -> Option<SkydimoRgb> {
    let mut hex = raw.trim();
    if let Some(stripped) = hex.strip_prefix('#') {
        hex = stripped;
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
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::{parse_color_array_field, sample_gradient, RainbowEffect, CUSTOM_PRESET};
    use crate::abi::SkydimoRgb;

    #[test]
    fn parses_custom_color_arrays() {
        let mut colors = Vec::new();
        assert!(parse_color_array_field(
            r##"{"colors":["#123456","#0af"]}"##,
            "colors",
            &mut colors
        ));
        assert_eq!(
            colors,
            vec![
                SkydimoRgb {
                    r: 18,
                    g: 52,
                    b: 86,
                },
                SkydimoRgb {
                    r: 0,
                    g: 170,
                    b: 255,
                },
            ]
        );
    }

    #[test]
    fn samples_wrapping_gradient() {
        let palette = [
            SkydimoRgb { r: 0, g: 0, b: 0 },
            SkydimoRgb {
                r: 100,
                g: 50,
                b: 0,
            },
        ];
        assert_eq!(sample_gradient(&palette, 0.0), palette[0]);
        assert_eq!(sample_gradient(&palette, 0.5), palette[1]);
        assert_eq!(sample_gradient(&palette, 1.0), palette[0]);
    }

    #[test]
    fn renders_into_host_buffer() {
        let mut effect = RainbowEffect::new();
        effect.update_params(r##"{"preset":0,"colors":["#ff0000","#00ff00"],"speed":1}"##);
        effect.resize(8, 1, 8);
        assert_eq!(effect.preset, CUSTOM_PRESET);

        let mut pixels = [SkydimoRgb::default(); 8];
        effect.tick(0.0, &mut pixels);

        assert!(pixels.iter().any(|pixel| pixel.r != 0 || pixel.g != 0));
    }
}
