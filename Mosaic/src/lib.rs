use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const REFERENCE_FPS: f64 = 60.0;
const DEFAULT_SPEED: f32 = 10.0;
const DEFAULT_RARITY: u32 = 10;
const DEFAULT_COLOR: SkydimoRgb = SkydimoRgb { r: 255, g: 0, b: 0 };

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct SkydimoRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
pub struct SkydimoHostApiV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoHardwareCandidateV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoDeviceInfoV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoOutputDefinitionV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoOutputFrameV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoLedColorV1 {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoEffectApiV1 {
    pub size: u32,
    pub create: Option<unsafe extern "C" fn(*const SkydimoHostApiV1, *mut *mut c_void) -> i32>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub resize: Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> i32>,
    pub update_params_json: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> i32>,
    pub tick: Option<unsafe extern "C" fn(*mut c_void, f64, *mut SkydimoRgb, usize) -> i32>,
    pub is_ready: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoControllerApiV1 {
    pub size: u32,
    pub create: Option<
        unsafe extern "C" fn(
            *const SkydimoHostApiV1,
            *const SkydimoHardwareCandidateV1,
            *mut *mut c_void,
        ) -> i32,
    >,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub validate: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub init: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub get_device_info: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoDeviceInfoV1) -> i32>,
    pub get_output_count: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
    pub get_output:
        Option<unsafe extern "C" fn(*mut c_void, usize, *mut SkydimoOutputDefinitionV1) -> i32>,
    pub update: Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputFrameV1, usize) -> i32>,
    pub set_output_leds_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, usize) -> i32>,
    pub update_output:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputDefinitionV1) -> i32>,
    pub disconnect: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoExtensionApiV1 {
    pub size: u32,
    pub create: Option<unsafe extern "C" fn(*const SkydimoHostApiV1, *mut *mut c_void) -> i32>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub start: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub stop: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub on_scan_devices: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub on_event_json:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *const c_char, usize) -> i32>,
    pub on_page_message_json: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> i32>,
    pub on_device_frame: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const SkydimoOutputFrameV1,
            usize,
        ) -> i32,
    >,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoPluginApiV1 {
    pub size: u32,
    pub abi_version: u32,
    pub kind_mask: u32,
    pub effect: SkydimoEffectApiV1,
    pub controller: SkydimoControllerApiV1,
    pub extension: SkydimoExtensionApiV1,
    pub shutdown_plugin: Option<unsafe extern "C" fn()>,
}

#[derive(Clone, Copy)]
struct Tile {
    color: SkydimoRgb,
    brightness: f32,
    decay_mult: f32,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            color: DEFAULT_COLOR,
            brightness: 0.0,
            decay_mult: 1.0,
        }
    }
}

struct MosaicEffect {
    speed: f32,
    rarity: u32,
    random_enabled: bool,
    palette: Vec<SkydimoRgb>,
    tiles: Vec<Tile>,
    last_elapsed: Option<f64>,
    rng: XorShift64,
}

impl Default for MosaicEffect {
    fn default() -> Self {
        Self {
            speed: DEFAULT_SPEED,
            rarity: DEFAULT_RARITY,
            random_enabled: false,
            palette: vec![DEFAULT_COLOR],
            tiles: Vec::new(),
            last_elapsed: None,
            rng: XorShift64::seeded(),
        }
    }
}

impl MosaicEffect {
    fn resize(&mut self, _width: u32, _height: u32, led_count: u32) {
        if led_count > 0 {
            self.ensure_tile_count(led_count as usize);
        }
    }

    fn update_params_json(&mut self, bytes: &[u8]) {
        if let Some(speed) = json_number(bytes, b"speed") {
            if speed.is_finite() {
                self.speed = (speed + 0.5).floor().clamp(1.0, 200.0);
            }
        }

        if let Some(rarity) = json_number(bytes, b"rarity") {
            if rarity.is_finite() {
                self.rarity = (rarity + 0.5).floor().clamp(10.0, 2000.0) as u32;
            }
        }

        if let Some(random_enabled) = json_bool(bytes, b"random") {
            self.random_enabled = random_enabled;
        }

        let mut next_palette = Vec::new();
        if json_color_palette(bytes, b"colors", &mut next_palette) && !next_palette.is_empty() {
            self.palette = next_palette;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        self.ensure_tile_count(pixels.len());
        let decay_step = 0.0005 * self.speed * self.delta_frames(elapsed_seconds);
        let rarity = self.rarity.max(1);

        for (idx, pixel) in pixels.iter_mut().enumerate() {
            if self.tiles[idx].brightness <= 0.0 {
                if self.rng.one_in(rarity) {
                    let decay_mult = 1.0 + self.rng.next_f32();
                    let color = self.spawn_color();
                    let tile = &mut self.tiles[idx];
                    tile.brightness = 1.0;
                    tile.decay_mult = decay_mult;
                    tile.color = color;
                } else {
                    self.tiles[idx].brightness = 0.0;
                    *pixel = SkydimoRgb::default();
                    continue;
                }
            }

            let tile = &mut self.tiles[idx];
            tile.brightness -= decay_step * tile.decay_mult;
            *pixel = if tile.brightness > 0.0 {
                scale_rgb(tile.color, tile.brightness)
            } else {
                SkydimoRgb::default()
            };
        }
    }

    fn ensure_tile_count(&mut self, count: usize) {
        self.tiles.resize(count, Tile::default());
    }

    fn delta_frames(&mut self, elapsed_seconds: f64) -> f32 {
        let mut delta_frames = 1.0f32;

        if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            if let Some(last) = self.last_elapsed {
                if elapsed_seconds >= last {
                    delta_frames = ((elapsed_seconds - last) * REFERENCE_FPS) as f32;
                    if delta_frames < 0.0 || !delta_frames.is_finite() {
                        delta_frames = 0.0;
                    }
                }
            }
            self.last_elapsed = Some(elapsed_seconds);
        } else {
            self.last_elapsed = None;
        }

        delta_frames
    }

    fn spawn_color(&mut self) -> SkydimoRgb {
        if self.random_enabled {
            return hsv_to_rgb(self.rng.next_f32() * 360.0, 1.0, 1.0);
        }

        if self.palette.is_empty() {
            return DEFAULT_COLOR;
        }
        let idx = self.rng.range_usize(self.palette.len());
        self.palette[idx]
    }
}

unsafe extern "C" fn mosaic_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(MosaicEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn mosaic_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<MosaicEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn mosaic_resize(
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

unsafe extern "C" fn mosaic_update_params_json(
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
        effect.update_params_json(bytes);
        0
    })
}

unsafe extern "C" fn mosaic_tick(
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

unsafe extern "C" fn mosaic_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the ABI declared by the plugin manifest.
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
                    create: Some(mosaic_create),
                    destroy: Some(mosaic_destroy),
                    resize: Some(mosaic_resize),
                    update_params_json: Some(mosaic_update_params_json),
                    tick: Some(mosaic_tick),
                    is_ready: Some(mosaic_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut MosaicEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<MosaicEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f32> {
    let start = json_value_start(bytes, key)?;
    let mut end = start;
    while end < bytes.len()
        && matches!(
            bytes[end],
            b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'
        )
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..end]).ok()?.parse().ok()
}

fn json_bool(bytes: &[u8], key: &[u8]) -> Option<bool> {
    let start = json_value_start(bytes, key)?;
    if bytes[start..].starts_with(b"true") {
        Some(true)
    } else if bytes[start..].starts_with(b"false") {
        Some(false)
    } else {
        None
    }
}

fn json_color_palette(bytes: &[u8], key: &[u8], out: &mut Vec<SkydimoRgb>) -> bool {
    let Some(mut pos) = json_value_start(bytes, key) else {
        return false;
    };
    if bytes.get(pos).copied() != Some(b'[') {
        return false;
    }
    pos += 1;

    loop {
        pos = skip_ascii_ws(bytes, pos);
        match bytes.get(pos).copied() {
            Some(b']') => return true,
            Some(b',') => {
                pos += 1;
                continue;
            }
            Some(b'"') => {
                let Some((value, next_pos)) = json_string_at(bytes, pos + 1) else {
                    return true;
                };
                if let Some(color) = parse_hex_color(value).map(normalize_palette_color) {
                    out.push(color);
                }
                pos = next_pos;
            }
            Some(_) => {
                let Some(next_pos) = find_next_array_separator(bytes, pos) else {
                    return true;
                };
                pos = next_pos;
            }
            None => return true,
        }
    }
}

fn json_value_start(bytes: &[u8], key: &[u8]) -> Option<usize> {
    let key_pos = find_json_key(bytes, key)?;
    let mut pos = key_pos + key.len() + 2;
    pos = skip_ascii_ws(bytes, pos);
    if bytes.get(pos).copied()? != b':' {
        return None;
    }
    Some(skip_ascii_ws(bytes, pos + 1))
}

fn find_json_key(bytes: &[u8], key: &[u8]) -> Option<usize> {
    if key.is_empty() || bytes.len() < key.len() + 2 {
        return None;
    }

    let last = bytes.len() - key.len() - 1;
    let mut pos = 0usize;
    while pos < last {
        if bytes[pos] == b'"'
            && bytes[pos + 1..].starts_with(key)
            && bytes.get(pos + key.len() + 1).copied() == Some(b'"')
        {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn json_string_at(bytes: &[u8], mut pos: usize) -> Option<(&[u8], usize)> {
    let start = pos;
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some((&bytes[start..pos], pos + 1));
        }
        pos += 1;
    }
    None
}

fn find_next_array_separator(bytes: &[u8], mut pos: usize) -> Option<usize> {
    while pos < bytes.len() {
        if matches!(bytes[pos], b',' | b']') {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn skip_ascii_ws(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

fn parse_hex_color(raw: &[u8]) -> Option<SkydimoRgb> {
    let mut digits = [0u8; 6];
    let mut len = 0usize;

    for &byte in raw {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'#' && len == 0 {
            continue;
        }
        if len == digits.len() {
            return None;
        }
        digits[len] = byte;
        len += 1;
    }

    match len {
        3 => Some(SkydimoRgb {
            r: parse_hex_nibble(digits[0])? * 17,
            g: parse_hex_nibble(digits[1])? * 17,
            b: parse_hex_nibble(digits[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: parse_hex_byte(digits[0], digits[1])?,
            g: parse_hex_byte(digits[2], digits[3])?,
            b: parse_hex_byte(digits[4], digits[5])?,
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

fn normalize_palette_color(color: SkydimoRgb) -> SkydimoRgb {
    let (h, s) = rgb_to_hs(color);
    hsv_to_rgb(h, s, 1.0)
}

fn rgb_to_hs(color: SkydimoRgb) -> (f32, f32) {
    let rn = color.r as f32 / 255.0;
    let gn = color.g as f32 / 255.0;
    let bn = color.b as f32 / 255.0;

    let max_channel = rn.max(gn).max(bn);
    let min_channel = rn.min(gn).min(bn);
    let delta = max_channel - min_channel;

    let h = if delta <= 0.0 {
        0.0
    } else if max_channel == rn {
        60.0 * ((gn - bn) / delta).rem_euclid(6.0)
    } else if max_channel == gn {
        60.0 * (((bn - rn) / delta) + 2.0)
    } else {
        60.0 * (((rn - gn) / delta) + 4.0)
    };

    let s = if max_channel > 0.0 {
        delta / max_channel
    } else {
        0.0
    };

    (h, s)
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

fn scale_rgb(color: SkydimoRgb, scale: f32) -> SkydimoRgb {
    if scale <= 0.0 {
        return SkydimoRgb::default();
    }
    if scale >= 1.0 {
        return color;
    }

    SkydimoRgb {
        r: scale_channel(color.r, scale),
        g: scale_channel(color.g, scale),
        b: scale_channel(color.b, scale),
    }
}

fn scale_channel(channel: u8, scale: f32) -> u8 {
    (channel as f32 * scale).round().clamp(0.0, 255.0) as u8
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
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
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
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

    fn next_f32(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 16_777_216.0
    }

    fn one_in(&mut self, rarity: u32) -> bool {
        self.next_u64().is_multiple_of(u64::from(rarity.max(1)))
    }

    fn range_usize(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            0
        } else {
            (self.next_u64() % upper_exclusive as u64) as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::{
        json_bool, json_color_palette, json_number, normalize_palette_color, parse_hex_color,
        skydimo_plugin_get_api, MosaicEffect, SkydimoPluginApiV1, SkydimoRgb, XorShift64,
        SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_and_normalizes_params() {
        let json = br##"{"speed":12.6,"rarity":42.2,"random":true,"colors":["#800000"," #808080 "]}"##;
        assert_eq!(json_number(json, b"speed"), Some(12.6));
        assert_eq!(json_number(json, b"rarity"), Some(42.2));
        assert_eq!(json_bool(json, b"random"), Some(true));

        let mut effect = MosaicEffect::default();
        effect.update_params_json(json);
        assert_eq!(effect.speed, 13.0);
        assert_eq!(effect.rarity, 42);
        assert!(effect.random_enabled);
        assert_eq!(effect.palette[0], SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(
            effect.palette[1],
            SkydimoRgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
    }

    #[test]
    fn parses_short_hex_and_internal_whitespace_like_lua() {
        assert_eq!(
            parse_hex_color(b" #0 a f "),
            Some(SkydimoRgb {
                r: 0,
                g: 170,
                b: 255
            })
        );
        assert_eq!(
            normalize_palette_color(SkydimoRgb {
                r: 16,
                g: 32,
                b: 48
            }),
            SkydimoRgb {
                r: 85,
                g: 170,
                b: 255
            }
        );
    }

    #[test]
    fn color_palette_ignores_invalid_entries_but_reports_present_array() {
        let mut colors = Vec::new();
        assert!(json_color_palette(
            br##"{"colors":["xyz","#00ff00",12]}"##,
            b"colors",
            &mut colors
        ));
        assert_eq!(colors, vec![SkydimoRgb { r: 0, g: 255, b: 0 }]);
    }

    #[test]
    fn forced_spawn_renders_a_lit_tile() {
        let mut effect = MosaicEffect {
            rarity: 1,
            rng: XorShift64::new(1),
            ..MosaicEffect::default()
        };
        let mut pixels = [SkydimoRgb::default(); 4];

        effect.tick(0.0, &mut pixels);

        assert!(pixels.iter().any(|pixel| pixel.r > 0));
        assert!(pixels.iter().all(|pixel| pixel.g == 0 && pixel.b == 0));
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
        assert_eq!(api.kind_mask, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }

    #[test]
    fn ffi_create_tick_destroy_round_trip() {
        let mut api = SkydimoPluginApiV1::default();
        unsafe {
            assert_eq!(
                skydimo_plugin_get_api(
                    SKYDIMO_NATIVE_C_ABI_VERSION,
                    std::ptr::null(),
                    &mut api,
                ),
                0
            );
        }

        let mut instance: *mut c_void = std::ptr::null_mut();
        unsafe {
            assert_eq!(api.effect.create.unwrap()(std::ptr::null(), &mut instance), 0);
            assert!(!instance.is_null());
            assert_eq!(api.effect.resize.unwrap()(instance, 4, 1, 4), 0);
            assert_eq!(
                api.effect.update_params_json.unwrap()(
                    instance,
                    br#"{"rarity":1}"#.as_ptr().cast::<i8>(),
                    br#"{"rarity":1}"#.len(),
                ),
                0
            );

            let mut pixels = [SkydimoRgb::default(); 4];
            assert_eq!(
                api.effect
                    .tick
                    .unwrap()(instance, 0.0, pixels.as_mut_ptr(), pixels.len()),
                0
            );
            assert_eq!(api.effect.is_ready.unwrap()(instance), 1);
            api.effect.destroy.unwrap()(instance);
        }
    }
}
