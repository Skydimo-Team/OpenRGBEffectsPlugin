mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: f32 = 50.0;
const MIN_SPEED: f32 = 10.0;
const MAX_SPEED: f32 = 100.0;
const DEFAULT_THICKNESS: f32 = 10.0;
const MIN_THICKNESS: f32 = 1.0;
const MAX_THICKNESS: f32 = 20.0;
const BREATH_RADIUS_SCALE: f64 = 0.35;
const BREATH_TIME_SCALE: f64 = 0.1;
const BLACK: SkydimoRgb = SkydimoRgb { r: 0, g: 0, b: 0 };

#[derive(Clone, Copy)]
struct Config {
    speed: f32,
    thickness: f32,
    random_enabled: bool,
    user_color: SkydimoRgb,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: DEFAULT_SPEED,
            thickness: DEFAULT_THICKNESS,
            random_enabled: false,
            user_color: SkydimoRgb { r: 255, g: 0, b: 0 },
        }
    }
}

#[derive(Default)]
struct GeometryCache {
    width: usize,
    height: usize,
    distances: Vec<f32>,
    thickness_scale: f32,
}

struct BreathingCircleEffect {
    config: Config,
    random_color: SkydimoRgb,
    last_cycle: i64,
    width: usize,
    height: usize,
    geometry: GeometryCache,
    rng: FastRng,
}

impl BreathingCircleEffect {
    fn new() -> Self {
        let mut rng = FastRng::new(seed_now());
        let random_color = random_rgb_color(&mut rng);

        Self {
            config: Config::default(),
            random_color,
            last_cycle: -1,
            width: 0,
            height: 1,
            geometry: GeometryCache::default(),
            rng,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1) as usize;
        self.height = height.max(1) as usize;
        self.rebuild_geometry(self.width, self.height);
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = parse_number_field(json, "speed") {
            self.config.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        }
        if let Some(thickness) = parse_number_field(json, "thickness") {
            self.config.thickness = thickness.clamp(MIN_THICKNESS, MAX_THICKNESS);
        }
        if let Some(random_enabled) = parse_bool_field(json, "random") {
            self.config.random_enabled = random_enabled;
        }
        if let Some(color) = parse_color_field(json, "color") {
            self.config.user_color = color;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let (width, height) = self.dimensions(pixels.len());
        self.ensure_geometry(width, height);

        let elapsed_seconds = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            elapsed_seconds
        } else {
            0.0
        };
        let theta = elapsed_seconds * self.config.speed as f64 * BREATH_TIME_SCALE;
        let progress = (BREATH_RADIUS_SCALE * (1.0 + theta.sin())) as f32;

        let cycle = ((theta + std::f64::consts::FRAC_PI_2) / std::f64::consts::TAU).floor() as i64;
        if cycle != self.last_cycle {
            self.last_cycle = cycle;
            self.random_color = random_rgb_color(&mut self.rng);
        }

        let active_color = if self.config.random_enabled {
            self.random_color
        } else {
            self.config.user_color
        };
        let thickness_norm = if self.geometry.thickness_scale > 0.0 {
            self.config.thickness * self.geometry.thickness_scale
        } else {
            1.0
        };
        let inner_edge = progress - thickness_norm;

        render_ring(
            pixels,
            &self.geometry.distances,
            progress,
            inner_edge,
            active_color,
        );
    }

    fn dimensions(&self, len: usize) -> (usize, usize) {
        if self.width == 0 || self.height == 0 {
            (len.max(1), 1)
        } else {
            (self.width.max(1), self.height.max(1))
        }
    }

    fn ensure_geometry(&mut self, width: usize, height: usize) {
        if self.geometry.width != width
            || self.geometry.height != height
            || self.geometry.distances.is_empty()
        {
            self.rebuild_geometry(width, height);
        }
    }

    fn rebuild_geometry(&mut self, width: usize, height: usize) {
        let width = width.max(1);
        let height = height.max(1);
        let Some(total) = width.checked_mul(height) else {
            self.geometry = GeometryCache {
                width,
                height,
                distances: Vec::new(),
                thickness_scale: 0.0,
            };
            return;
        };

        let w_dim = width.saturating_sub(1);
        let h_dim = height.saturating_sub(1);
        let avg_dim = w_dim + h_dim;
        let thickness_scale = if avg_dim > 0 {
            2.0 / avg_dim as f32
        } else {
            0.0
        };

        self.geometry.width = width;
        self.geometry.height = height;
        self.geometry.thickness_scale = thickness_scale;
        self.geometry.distances.clear();
        self.geometry.distances.reserve(total);

        for y in 0..height {
            let ny = if h_dim > 0 {
                y as f32 / h_dim as f32
            } else {
                0.5
            };
            let dy = 0.5 - ny;
            let dy2 = dy * dy;

            for x in 0..width {
                let nx = if w_dim > 0 {
                    x as f32 / w_dim as f32
                } else {
                    0.5
                };
                let dx = 0.5 - nx;
                self.geometry
                    .distances
                    .push(dx.mul_add(dx, dy2).sqrt().min(1.0));
            }
        }
    }
}

fn render_ring(
    pixels: &mut [SkydimoRgb],
    distances: &[f32],
    progress: f32,
    inner_edge: f32,
    active_color: SkydimoRgb,
) {
    let active_len = pixels.len().min(distances.len());
    for (pixel, &distance) in pixels[..active_len].iter_mut().zip(distances.iter()) {
        *pixel = if distance <= progress && distance >= inner_edge {
            active_color
        } else {
            BLACK
        };
    }

    if active_len < pixels.len() {
        pixels[active_len..].fill(BLACK);
    }
}

#[derive(Clone, Copy)]
struct FastRng {
    state: u64,
}

impl FastRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x.max(1);
        x
    }

    fn next_unit(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 0x00FF_FFFFu32 as f32
    }
}

unsafe extern "C" fn breathing_circle_create(
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

        let effect = Box::new(BreathingCircleEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn breathing_circle_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<BreathingCircleEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn breathing_circle_resize(
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

unsafe extern "C" fn breathing_circle_update_params_json(
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

unsafe extern "C" fn breathing_circle_tick(
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

unsafe extern "C" fn breathing_circle_is_ready(instance: *mut c_void) -> i32 {
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
                    create: Some(breathing_circle_create),
                    destroy: Some(breathing_circle_destroy),
                    resize: Some(breathing_circle_resize),
                    update_params_json: Some(breathing_circle_update_params_json),
                    tick: Some(breathing_circle_tick),
                    is_ready: Some(breathing_circle_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut BreathingCircleEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<BreathingCircleEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
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

fn parse_color_field(json: &str, key: &str) -> Option<SkydimoRgb> {
    parse_hex_color(json_value_slice(json, key)?)
}

fn json_value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = json_value_after_colon(json, key)?.trim_start();

    if raw.starts_with('"') {
        let (value, _) = read_json_string(raw)?;
        return Some(value.trim());
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

fn json_value_after_colon<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    Some(&after_key[colon + 1..])
}

fn read_json_string(raw: &str) -> Option<(&str, &str)> {
    let body = raw.trim_start().strip_prefix('"')?;
    let mut escaped = false;
    for (idx, ch) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some((&body[..idx], &body[idx + 1..])),
            _ => {}
        }
    }
    None
}

fn parse_hex_color(value: &str) -> Option<SkydimoRgb> {
    let hex = value.trim().strip_prefix('#').unwrap_or_else(|| value.trim());
    let bytes = hex.as_bytes();

    match bytes.len() {
        3 => Some(SkydimoRgb {
            r: hex_nibble(bytes[0])? * 17,
            g: hex_nibble(bytes[1])? * 17,
            b: hex_nibble(bytes[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: hex_pair(bytes[0], bytes[1])?,
            g: hex_pair(bytes[2], bytes[3])?,
            b: hex_pair(bytes[4], bytes[5])?,
        }),
        _ => None,
    }
}

fn hex_pair(hi: u8, lo: u8) -> Option<u8> {
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

fn random_rgb_color(rng: &mut FastRng) -> SkydimoRgb {
    hsv_to_rgb(rng.next_unit() * 360.0, 1.0, 1.0)
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

fn seed_now() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x5A17_2026_0508);
    splitmix64(nanos ^ 0x9E37_79B9_7F4A_7C15)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = value;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_bool_field, parse_color_field, parse_number_field, skydimo_plugin_get_api,
        BreathingCircleEffect,
    };
    use crate::abi::{
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_params_without_json_allocation() {
        let json = r##"{
            "speed": 75,
            "thickness": 6,
            "random": true,
            "color": "#12aBf0"
        }"##;

        assert_eq!(parse_number_field(json, "speed"), Some(75.0));
        assert_eq!(parse_number_field(json, "thickness"), Some(6.0));
        assert_eq!(parse_bool_field(json, "random"), Some(true));
        assert_eq!(
            parse_color_field(json, "color"),
            Some(SkydimoRgb {
                r: 0x12,
                g: 0xab,
                b: 0xf0
            })
        );
    }

    #[test]
    fn renders_ring_into_existing_buffer() {
        let mut effect = BreathingCircleEffect::new();
        effect.resize(3, 3);
        effect.update_params(r##"{"color":"#00FF00","random":false,"thickness":1}"##);

        let mut pixels = [SkydimoRgb::default(); 9];
        effect.tick(0.0, &mut pixels);

        assert_eq!(
            pixels[4],
            SkydimoRgb {
                r: 0,
                g: 255,
                b: 0
            }
        );
        assert!(pixels.iter().any(|pixel| *pixel == SkydimoRgb::default()));
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
        assert_eq!(
            api.kind_mask & SKYDIMO_PLUGIN_KIND_EFFECT,
            SKYDIMO_PLUGIN_KIND_EFFECT
        );
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }
}
