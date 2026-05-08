use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

mod abi;

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: f64 = 10.0;
const MIN_SPEED: f64 = 1.0;
const MAX_SPEED: f64 = 20.0;
const TIME_SCALE: f64 = 0.1;

struct SwapEffect {
    speed: f64,
    random_enabled: bool,
    user_colors: [SkydimoRgb; 2],
    random_colors: [SkydimoRgb; 2],
    current_colors: [SkydimoRgb; 2],
    time_acc: f64,
    progress: f64,
    dir: bool,
    old_dir: bool,
    last_t: Option<f64>,
    width: usize,
    height: usize,
    rng: FastRng,
}

impl SwapEffect {
    fn new() -> Self {
        let mut rng = FastRng::new(seed_now());
        let user_colors = [
            SkydimoRgb { r: 255, g: 0, b: 0 },
            SkydimoRgb { r: 0, g: 0, b: 255 },
        ];
        let random_colors = [random_rgb_color(&mut rng), random_rgb_color(&mut rng)];

        Self {
            speed: DEFAULT_SPEED,
            random_enabled: false,
            user_colors,
            random_colors,
            current_colors: user_colors,
            time_acc: 0.0,
            progress: 0.0,
            dir: false,
            old_dir: false,
            last_t: None,
            width: 0,
            height: 1,
            rng,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1) as usize;
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = parse_number_field(json, "speed") {
            self.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        }

        if let Some(colors) = parse_color_pair_field(json, "colors") {
            self.user_colors = colors;
        }

        if let Some(random_enabled) = parse_bool_field(json, "random") {
            let was_random = self.random_enabled;
            self.random_enabled = random_enabled;
            if self.random_enabled && !was_random {
                self.random_colors = [
                    random_rgb_color(&mut self.rng),
                    random_rgb_color(&mut self.rng),
                ];
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let (width, height) = self.dimensions(pixels.len());
        let x = if self.dir {
            self.progress
        } else {
            1.0 - self.progress
        };
        let threshold = x * (width as f64 + 1.0);
        render_swap(
            pixels,
            width,
            height,
            threshold,
            self.current_colors[0],
            self.current_colors[1],
        );

        let delta = self.next_delta(elapsed_seconds);
        self.time_acc += TIME_SCALE * self.speed * delta;

        let whole = self.time_acc.floor();
        self.progress = self.time_acc - whole;
        self.dir = (whole as u64 & 1) == 1;

        self.current_colors = if self.random_enabled {
            self.random_colors
        } else {
            self.user_colors
        };

        if !self.old_dir && self.dir {
            self.random_colors[0] = random_rgb_color(&mut self.rng);
        } else if self.old_dir && !self.dir {
            self.random_colors[1] = random_rgb_color(&mut self.rng);
        }
        self.old_dir = self.dir;
    }

    fn dimensions(&self, len: usize) -> (usize, usize) {
        if self.width == 0 || self.height == 0 {
            (len.max(1), 1)
        } else {
            (self.width.max(1), self.height.max(1))
        }
    }

    fn next_delta(&mut self, elapsed_seconds: f64) -> f64 {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return 0.0;
        }

        let delta = match self.last_t {
            Some(last_t) if elapsed_seconds >= last_t => elapsed_seconds - last_t,
            _ => elapsed_seconds,
        };
        self.last_t = Some(elapsed_seconds);
        delta
    }
}

fn render_swap(
    pixels: &mut [SkydimoRgb],
    width: usize,
    height: usize,
    threshold: f64,
    color1: SkydimoRgb,
    color2: SkydimoRgb,
) {
    let active_len = pixels.len().min(width.saturating_mul(height));
    if active_len == 0 {
        return;
    }

    let split = threshold.floor().clamp(0.0, width as f64) as usize;
    for row in pixels[..active_len].chunks_mut(width) {
        let row_split = split.min(row.len());
        let (left, right) = row.split_at_mut(row_split);
        left.fill(color1);
        right.fill(color2);
    }

    if active_len < pixels.len() {
        pixels[active_len..].fill(color2);
    }
}

#[derive(Clone, Copy)]
struct FastRng {
    state: u64,
}

impl FastRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.max(1),
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

    fn next_unit(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 0x00FF_FFFFu32 as f32
    }
}

unsafe extern "C" fn swap_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(SwapEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn swap_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<SwapEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn swap_resize(
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

unsafe extern "C" fn swap_update_params_json(
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

unsafe extern "C" fn swap_tick(
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

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn swap_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer to `SkydimoPluginApiV1`.
/// The host must pass a supported ABI version and keep callback pointers valid
/// for the lifetime of instances it creates through the returned API.
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
                    create: Some(swap_create),
                    destroy: Some(swap_destroy),
                    resize: Some(swap_resize),
                    update_params_json: Some(swap_update_params_json),
                    tick: Some(swap_tick),
                    is_ready: Some(swap_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut SwapEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<SwapEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn parse_number_field(json: &str, key: &str) -> Option<f64> {
    json_value_slice(json, key)?.parse::<f64>().ok()
}

fn parse_bool_field(json: &str, key: &str) -> Option<bool> {
    match json_value_slice(json, key)? {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_color_pair_field(json: &str, key: &str) -> Option<[SkydimoRgb; 2]> {
    let mut rest = json_value_after_colon(json, key)?.trim_start();
    rest = rest.strip_prefix('[')?;

    let mut out = [SkydimoRgb::default(); 2];
    let mut count = 0usize;
    while count < 2 {
        rest = rest.trim_start();
        if rest.starts_with(']') {
            break;
        }

        let (raw, after) = read_json_string(rest)?;
        out[count] = parse_hex_color(raw)?;
        count += 1;

        rest = after.trim_start();
        if let Some(after_comma) = rest.strip_prefix(',') {
            rest = after_comma;
        } else if rest.starts_with(']') {
            break;
        } else {
            return None;
        }
    }

    (count == 2).then_some(out)
}

fn json_value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = json_value_after_colon(json, key)?.trim_start();

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
        3 => {
            let r = hex_nibble(bytes[0])?;
            let g = hex_nibble(bytes[1])?;
            let b = hex_nibble(bytes[2])?;
            Some(SkydimoRgb {
                r: r * 17,
                g: g * 17,
                b: b * 17,
            })
        }
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
        .unwrap_or(0x5A17_2026_0507);
    splitmix64(nanos ^ 0x9E37_79B9_7F4A_7C15)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = value;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
