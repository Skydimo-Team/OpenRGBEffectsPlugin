mod abi;

use std::ffi::{c_char, c_void};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const TIME_PERIOD: f64 = 18.0;

struct ZigzagEffect {
    speed: f64,
    color_mode: ColorMode,
    color: SkydimoRgb,
    width: usize,
    height: usize,
    time_acc: f64,
    progress: f64,
    last_elapsed: Option<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    Rainbow,
    Custom,
}

impl Default for ZigzagEffect {
    fn default() -> Self {
        Self {
            speed: 10.0,
            color_mode: ColorMode::Rainbow,
            color: SkydimoRgb { r: 255, g: 0, b: 0 },
            width: 0,
            height: 1,
            time_acc: 0.0,
            progress: 0.0,
            last_elapsed: None,
        }
    }
}

impl ZigzagEffect {
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1) as usize;
        self.height = height.max(1) as usize;
    }

    fn update_params_json(&mut self, json: &str) {
        if let Some(speed) = parse_json_number(json, "speed") {
            self.speed = speed.clamp(1.0, 20.0);
        }
        if let Some(color_mode) = parse_json_number(json, "color_mode") {
            match (color_mode + 0.5).floor() as i32 {
                0 => self.color_mode = ColorMode::Rainbow,
                1 => self.color_mode = ColorMode::Custom,
                _ => {}
            }
        }
        if let Some(color) = parse_json_string(json, "color").and_then(parse_hex_color) {
            self.color = color;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            self.advance_time(elapsed_seconds);
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width.max(1)
        };
        let height = self.height.max(1);
        let grid_len = width.saturating_mul(height);
        let render_len = pixels.len().min(grid_len);

        if render_len == 0 || self.progress <= 0.0 || !self.progress.is_finite() {
            pixels.fill(SkydimoRgb::default());
            self.advance_time(elapsed_seconds);
            return;
        }

        let position_scale = 1.0 / (grid_len as f64 * self.progress);
        let hue_time = -100.0 * self.time_acc;
        let color_mode = self.color_mode;
        let color = self.color;
        let mut idx = 0usize;

        for y in 0..height {
            if idx >= render_len {
                break;
            }
            for x in 0..width {
                if idx >= render_len {
                    break;
                }

                let position = if x & 1 == 0 {
                    y + x * height
                } else {
                    (height - y - 1) + x * height
                };
                let distance = position as f64 * position_scale;

                pixels[idx] = if distance < 1.0 {
                    let brightness = distance * distance * distance;
                    match color_mode {
                        ColorMode::Rainbow => {
                            let hue = trunc_toward_zero(brightness * 360.0 + hue_time)
                                .rem_euclid(360) as f32;
                            hsv_to_rgb(hue, 1.0, brightness as f32)
                        }
                        ColorMode::Custom => scale_rgb(color, brightness),
                    }
                } else {
                    SkydimoRgb::default()
                };
                idx += 1;
            }
        }

        if render_len < pixels.len() {
            pixels[render_len..].fill(SkydimoRgb::default());
        }

        self.advance_time(elapsed_seconds);
    }

    fn advance_time(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return;
        }

        let dt = match self.last_elapsed {
            Some(last) if elapsed_seconds >= last => elapsed_seconds - last,
            _ => elapsed_seconds,
        };
        self.last_elapsed = Some(elapsed_seconds);

        if dt <= 0.0 || !dt.is_finite() {
            return;
        }

        self.time_acc = (self.time_acc + 0.01 * self.speed * dt).rem_euclid(TIME_PERIOD);
        self.progress = 2.0 * self.time_acc.fract();
    }
}

unsafe extern "C" fn zigzag_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    if out_instance.is_null() {
        return -1;
    }

    let effect = Box::new(ZigzagEffect::default());
    unsafe {
        *out_instance = Box::into_raw(effect).cast::<c_void>();
    }
    0
}

unsafe extern "C" fn zigzag_destroy(instance: *mut c_void) {
    if !instance.is_null() {
        unsafe {
            drop(Box::from_raw(instance.cast::<ZigzagEffect>()));
        }
    }
}

unsafe extern "C" fn zigzag_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    _led_count: u32,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    effect.resize(width, height);
    0
}

unsafe extern "C" fn zigzag_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
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
    effect.update_params_json(json);
    0
}

unsafe extern "C" fn zigzag_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    if buffer.is_null() && len > 0 {
        return -2;
    }
    if len == 0 {
        effect.advance_time(elapsed_seconds);
        return 0;
    }

    let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
    effect.tick(elapsed_seconds, pixels);
    0
}

unsafe extern "C" fn zigzag_is_ready(instance: *mut c_void) -> i32 {
    if instance.is_null() {
        -1
    } else {
        1
    }
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
/// The host must pass the ABI version it expects in `requested_abi_version`.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
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
                create: Some(zigzag_create),
                destroy: Some(zigzag_destroy),
                resize: Some(zigzag_resize),
                update_params_json: Some(zigzag_update_params_json),
                tick: Some(zigzag_tick),
                is_ready: Some(zigzag_is_ready),
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
}

unsafe fn effect_mut(instance: *mut c_void) -> Option<&'static mut ZigzagEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<ZigzagEffect>() })
    }
}

fn parse_json_number(json: &str, key: &str) -> Option<f64> {
    let mut raw = json_value_after_key(json, key)?.trim_start();
    if raw.starts_with('"') {
        raw = &raw[1..];
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
    raw[..end].trim().parse::<f64>().ok()
}

fn parse_json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = json_value_after_key(json, key)?.trim_start();
    let raw = raw.strip_prefix('"')?;
    let end = raw
        .as_bytes()
        .iter()
        .position(|byte| *byte == b'"')?;
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

fn parse_hex_color(value: &str) -> Option<SkydimoRgb> {
    let hex = value.trim().strip_prefix('#').unwrap_or_else(|| value.trim());
    let bytes = hex.as_bytes();
    if bytes.len() != 6 {
        return None;
    }

    Some(SkydimoRgb {
        r: hex_byte(bytes[0], bytes[1])?,
        g: hex_byte(bytes[2], bytes[3])?,
        b: hex_byte(bytes[4], bytes[5])?,
    })
}

fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((hex_nibble(hi)? << 4) | hex_nibble(lo)?)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn trunc_toward_zero(value: f64) -> i32 {
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

fn scale_rgb(color: SkydimoRgb, scale: f64) -> SkydimoRgb {
    SkydimoRgb {
        r: to_u8(color.r as f32 * scale as f32),
        g: to_u8(color.g as f32 * scale as f32),
        b: to_u8(color.b as f32 * scale as f32),
    }
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_params_without_allocating_json_values() {
        let json = r##"{"speed":15,"color_mode":1,"color":"#7F20AA"}"##;
        assert_eq!(parse_json_number(json, "speed"), Some(15.0));
        assert_eq!(parse_json_number(json, "color_mode"), Some(1.0));
        let color = parse_json_string(json, "color")
            .and_then(parse_hex_color)
            .expect("color should parse");
        assert_eq!((color.r, color.g, color.b), (127, 32, 170));
    }

    #[test]
    fn renders_black_before_first_progress_advance() {
        let mut effect = ZigzagEffect::default();
        effect.resize(4, 2);
        let mut pixels = vec![SkydimoRgb { r: 1, g: 2, b: 3 }; 8];
        effect.tick(0.016, &mut pixels);
        assert!(pixels.iter().all(|px| px.r == 0 && px.g == 0 && px.b == 0));
        assert!(effect.progress > 0.0);
    }

    #[test]
    fn custom_mode_scales_user_color_along_zigzag_path() {
        let mut effect = ZigzagEffect {
            color_mode: ColorMode::Custom,
            color: SkydimoRgb {
                r: 200,
                g: 100,
                b: 50,
            },
            width: 4,
            height: 2,
            progress: 1.0,
            ..ZigzagEffect::default()
        };
        let mut pixels = vec![SkydimoRgb::default(); 8];
        effect.tick(0.0, &mut pixels);

        assert_eq!((pixels[0].r, pixels[0].g, pixels[0].b), (0, 0, 0));
        assert!(pixels[7].r > pixels[1].r);
        assert!(pixels[7].g > pixels[1].g);
    }
}
