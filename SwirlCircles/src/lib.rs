use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

mod abi;

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const HUE_CYCLE_RATE: f64 = 60.0;
const DEFAULT_COLOR_1: Hsv = Hsv {
    h: 0.0,
    s: 255.0,
    v: 255.0,
};
const DEFAULT_COLOR_2: Hsv = Hsv {
    h: 180.0,
    s: 255.0,
    v: 255.0,
};

#[derive(Clone, Copy)]
struct Hsv {
    h: f64,
    s: f64,
    v: f64,
}

#[derive(Clone, Copy)]
struct UnitRgb {
    r: f64,
    g: f64,
    b: f64,
}

struct SwirlCirclesEffect {
    speed: f64,
    glow: f64,
    radius: f64,
    reverse: bool,
    random_enabled: bool,
    hsv1: Hsv,
    hsv2: Hsv,
    width: usize,
    height: usize,
}

unsafe extern "C" fn swirl_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    ffi_status(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(SwirlCirclesEffect {
            speed: 50.0,
            glow: 50.0,
            radius: 0.0,
            reverse: false,
            random_enabled: true,
            hsv1: DEFAULT_COLOR_1,
            hsv2: DEFAULT_COLOR_2,
            width: 0,
            height: 1,
        });

        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn swirl_destroy(instance: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<SwirlCirclesEffect>()));
            }
        }
    }));
}

unsafe extern "C" fn swirl_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    ffi_status(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };

        if width == 0 || height == 0 {
            effect.width = led_count.max(1) as usize;
            effect.height = 1;
        } else {
            effect.width = width as usize;
            effect.height = height as usize;
        }
        0
    })
}

unsafe extern "C" fn swirl_update_params_json(
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

        if let Some(speed) = json_number(json, "speed") {
            effect.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(glow) = json_number(json, "glow") {
            effect.glow = glow.clamp(1.0, 100.0);
        }
        if let Some(radius) = json_number(json, "radius") {
            effect.radius = radius.round().clamp(0.0, 100.0);
        }
        if let Some(reverse) = json_bool(json, "reverse") {
            effect.reverse = reverse;
        }
        if let Some(random) = json_bool(json, "random") {
            let was_random = effect.random_enabled;
            effect.random_enabled = random;
            if random && !was_random {
                effect.hsv1 = DEFAULT_COLOR_1;
                effect.hsv2 = DEFAULT_COLOR_2;
            }
        }

        if !effect.random_enabled {
            if let Some(color) = json_string(json, "color1") {
                if let Some((r, g, b)) = parse_hex_rgb(color) {
                    effect.hsv1 = rgb_to_hsv_int(r, g, b);
                }
            }
            if let Some(color) = json_string(json, "color2") {
                if let Some((r, g, b)) = parse_hex_rgb(color) {
                    effect.hsv2 = rgb_to_hsv_int(r, g, b);
                }
            }
        }

        0
    })
}

unsafe extern "C" fn swirl_tick(
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
            return -1;
        }
        if len == 0 {
            return 0;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.render(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn swirl_is_ready(instance: *mut c_void) -> i32 {
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
                    create: Some(swirl_create),
                    destroy: Some(swirl_destroy),
                    resize: Some(swirl_resize),
                    update_params_json: Some(swirl_update_params_json),
                    tick: Some(swirl_tick),
                    is_ready: Some(swirl_is_ready),
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

impl SwirlCirclesEffect {
    fn render(&self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        let width = if self.width == 0 { pixels.len() } else { self.width.max(1) };
        let height = self.height.max(1);
        let total = pixels.len().min(width.saturating_mul(height));
        if total == 0 {
            return;
        }

        let mut progress = elapsed_seconds * 0.1 * self.speed;
        if self.reverse {
            progress = -progress;
        }

        let width_f = width as f64;
        let height_f = height as f64;
        let hx = 0.5 * width_f;
        let hy = 0.5 * height_f;
        let x1 = hx + hx * progress.cos();
        let y1 = hy + hy * progress.sin();
        let x2 = width_f - x1;
        let y2 = height_f - y1;
        let inv_dist_denom = 1.0 / (width_f + height_f).max(1.0);
        let glow_exp = 0.01 * self.glow;
        let radius = self.radius;

        let (hsv1, hsv2) = if self.random_enabled {
            let hue = (elapsed_seconds * HUE_CYCLE_RATE).rem_euclid(360.0);
            (
                Hsv {
                    h: hue,
                    s: 255.0,
                    v: 255.0,
                },
                Hsv {
                    h: (hue + 180.0).rem_euclid(360.0),
                    s: 255.0,
                    v: 255.0,
                },
            )
        } else {
            (self.hsv1, self.hsv2)
        };

        let unit1 = hsv_unit_rgb(hsv1.h, hsv1.s / 255.0);
        let unit2 = hsv_unit_rgb(hsv2.h, hsv2.s / 255.0);

        let mut idx = 0usize;
        for y in 0..height {
            if idx >= total {
                break;
            }
            let y = y as f64;
            let dy1 = y1 - y;
            let dy2 = y2 - y;
            for x in 0..width {
                if idx >= total {
                    break;
                }
                let x = x as f64;

                let dx1 = x1 - x;
                let distance1 = (dx1 * dx1 + dy1 * dy1).sqrt();
                let value1 = attenuated_value(hsv1.v, distance1, radius, inv_dist_denom, glow_exp);
                let rgb1 = unit_rgb_to_value(unit1, value1);

                let dx2 = x2 - x;
                let distance2 = (dx2 * dx2 + dy2 * dy2).sqrt();
                let value2 = attenuated_value(hsv2.v, distance2, radius, inv_dist_denom, glow_exp);
                let rgb2 = unit_rgb_to_value(unit2, value2);

                pixels[idx] = screen_blend(rgb1, rgb2);
                idx += 1;
            }
        }
    }
}

fn attenuated_value(v: f64, distance: f64, radius: f64, inv_dist_denom: f64, glow_exp: f64) -> u8 {
    if distance < radius {
        return v.clamp(0.0, 255.0) as u8;
    }

    let pct = (distance * inv_dist_denom).powf(glow_exp);
    (v * (1.0 - pct)).floor().clamp(0.0, 255.0) as u8
}

fn screen_blend(a: SkydimoRgb, b: SkydimoRgb) -> SkydimoRgb {
    SkydimoRgb {
        r: screen_channel(a.r, b.r),
        g: screen_channel(a.g, b.g),
        b: screen_channel(a.b, b.b),
    }
}

fn screen_channel(a: u8, b: u8) -> u8 {
    let inv_a = 255u16 - a as u16;
    let inv_b = 255u16 - b as u16;
    255u8.saturating_sub(((inv_a * inv_b) >> 8) as u8)
}

fn unit_rgb_to_value(unit: UnitRgb, value: u8) -> SkydimoRgb {
    let value = value as f64;
    SkydimoRgb {
        r: to_u8(unit.r * value),
        g: to_u8(unit.g * value),
        b: to_u8(unit.b * value),
    }
}

fn hsv_unit_rgb(h: f64, s: f64) -> UnitRgb {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let c = s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = 1.0 - c;

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

    UnitRgb {
        r: r + m,
        g: g + m,
        b: b + m,
    }
}

fn rgb_to_hsv_int(r: u8, g: u8, b: u8) -> Hsv {
    let r_f = r as f64;
    let g_f = g as f64;
    let b_f = b as f64;
    let max_c = r_f.max(g_f).max(b_f);
    let min_c = r_f.min(g_f).min(b_f);
    let delta = max_c - min_c;

    if max_c == 0.0 {
        return Hsv {
            h: 0.0,
            s: 0.0,
            v: 0.0,
        };
    }

    let s = (delta * 255.0 / max_c + 0.5).floor();
    let mut h = if delta == 0.0 {
        0.0
    } else if max_c == r_f {
        60.0 * ((g_f - b_f) / delta)
    } else if max_c == g_f {
        60.0 * ((b_f - r_f) / delta + 2.0)
    } else {
        60.0 * ((r_f - g_f) / delta + 4.0)
    };
    if h < 0.0 {
        h += 360.0;
    }

    Hsv {
        h: (h + 0.5).floor().rem_euclid(360.0),
        s,
        v: max_c,
    }
}

fn parse_hex_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() == 3 {
        let mut chars = hex.chars();
        let r = chars.next()?;
        let g = chars.next()?;
        let b = chars.next()?;
        return Some((hex_pair(r, r)?, hex_pair(g, g)?, hex_pair(b, b)?));
    }
    if hex.len() != 6 || !hex.bytes().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some((
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ))
}

fn hex_pair(hi: char, lo: char) -> Option<u8> {
    let hi = hi.to_digit(16)?;
    let lo = lo.to_digit(16)?;
    Some(((hi << 4) | lo) as u8)
}

fn json_number(json: &str, key: &str) -> Option<f64> {
    let value = json_value_start(json, key)?;
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
    value[..end].trim().parse::<f64>().ok()
}

fn json_bool(json: &str, key: &str) -> Option<bool> {
    let value = json_value_start(json, key)?;
    if value.starts_with("true") {
        Some(true)
    } else if value.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let value = json_value_start(json, key)?;
    let rest = value.strip_prefix('"')?;
    let mut escaped = false;
    for (idx, ch) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return Some(&rest[..idx]);
        }
    }
    None
}

fn json_value_start<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(needle.as_str())?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

fn to_u8(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn ffi_status(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-255)
}

fn effect_mut(instance: *mut c_void) -> Option<&'static mut SwirlCirclesEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<SwirlCirclesEffect>() })
    }
}
