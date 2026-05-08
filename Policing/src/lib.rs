mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: f32 = 50.0;
const DEFAULT_WIDTH: f32 = 20.0;
const DEFAULT_COLOR: SkydimoRgb = SkydimoRgb { r: 255, g: 0, b: 0 };

struct PolicingEffect {
    speed: f32,
    visor_width: f32,
    random_enabled: bool,
    user_color: SkydimoRgb,
    progress: f32,
    step: bool,
    last_step: bool,
    flash_length: f32,
    last_elapsed: f64,
    width: usize,
    height: usize,
    row_cache: Vec<SkydimoRgb>,
    rng: XorShift64,
}

impl PolicingEffect {
    fn new() -> Self {
        Self {
            speed: DEFAULT_SPEED,
            visor_width: DEFAULT_WIDTH,
            random_enabled: false,
            user_color: DEFAULT_COLOR,
            progress: 0.0,
            step: false,
            last_step: false,
            flash_length: 1.0,
            last_elapsed: 0.0,
            width: 0,
            height: 1,
            row_cache: Vec::new(),
            rng: XorShift64::seeded(),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = led_count.max(1) as usize;
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params_json(&mut self, bytes: &[u8]) {
        if let Some(speed) = json_number(bytes, b"speed") {
            if speed.is_finite() {
                self.speed = speed.clamp(1.0, 100.0);
            }
        }

        if let Some(width) = json_number(bytes, b"width") {
            if width.is_finite() {
                self.visor_width = width.clamp(1.0, 100.0);
            }
        }

        if let Some(random_enabled) = json_bool(bytes, b"random") {
            self.random_enabled = random_enabled;
        }

        if let Some(color) = json_string(bytes, b"color") {
            if let Some(rgb) = parse_hex_color(color) {
                self.user_color = rgb;
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let current_elapsed = if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            elapsed_seconds
        } else {
            0.0
        };
        let mut delta = current_elapsed - self.last_elapsed;
        if delta < 0.0 {
            delta = 0.0;
        }
        self.last_elapsed = current_elapsed;

        self.progress += 0.01 * self.speed * delta as f32;

        if self.flash_length < 0.0 {
            self.flash_length = 1.0;
            self.last_step = self.step;
        }

        let p = self.progress - self.progress.floor();
        self.step = p < 0.5;
        let p_step = if self.step { 2.0 * p } else { 2.0 * (1.0 - p) };
        let color = self.active_color();

        if self.last_step != self.step {
            fill_rgb(pixels, color);
            self.flash_length -= 0.03 * self.speed * delta as f32;
            return;
        }

        self.render_scan(pixels, p_step, color);
    }

    fn active_color(&mut self) -> SkydimoRgb {
        if self.random_enabled {
            hsv_to_rgb(self.rng.next_f32() * 360.0, 1.0, 1.0)
        } else {
            self.user_color
        }
    }

    fn render_scan(&mut self, pixels: &mut [SkydimoRgb], p_step: f32, color: SkydimoRgb) {
        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width
        }
        .max(1);
        let height = self.height.max(1);
        let width_fraction = 0.01 * self.visor_width;

        self.row_cache.resize(width, SkydimoRgb::default());
        for x in 0..width {
            self.row_cache[x] = self.sample_color(x, width, width_fraction, p_step, color);
        }

        let mut offset = 0usize;
        for _ in 0..height {
            if offset >= pixels.len() {
                return;
            }
            let copy_len = width.min(pixels.len() - offset);
            pixels[offset..offset + copy_len].copy_from_slice(&self.row_cache[..copy_len]);
            offset += copy_len;
        }

        if offset < pixels.len() {
            pixels[offset..].fill(SkydimoRgb::default());
        }
    }

    #[inline]
    fn sample_color(
        &self,
        index: usize,
        count: usize,
        width_fraction: f32,
        p_step: f32,
        color: SkydimoRgb,
    ) -> SkydimoRgb {
        let raw_count = count.max(1) as f32;
        let w = (1.5 / raw_count).max(width_fraction);
        let sample_count = if count <= 1 { 2.0 } else { raw_count };
        let x_step = p_step * (1.0 + 4.0 * w) - 1.5 * w;
        let x = index as f32 / (sample_count - 1.0);
        let dist = x_step - x;

        if dist < 0.0 {
            let l = clamp01((w + dist) / w);
            if self.step {
                SkydimoRgb::default()
            } else {
                scale_color(color, l)
            }
        } else if dist > w {
            let l = clamp01(1.0 - ((dist - w) / w));
            if self.step {
                scale_color(color, l)
            } else {
                SkydimoRgb::default()
            }
        } else {
            let interp = clamp01((w - dist) / w);
            if self.step {
                lerp_rgb(color, SkydimoRgb::default(), interp)
            } else {
                lerp_rgb(SkydimoRgb::default(), color, interp)
            }
        }
    }
}

unsafe extern "C" fn policing_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host = unsafe { &*host };
            if host.abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(PolicingEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn policing_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<PolicingEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn policing_resize(
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

unsafe extern "C" fn policing_update_params_json(
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

unsafe extern "C" fn policing_tick(
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

unsafe extern "C" fn policing_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a host-compatible
/// `SkydimoPluginApiV1`. The host passes the ABI version declared in
/// `manifest.json`.
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
                    create: Some(policing_create),
                    destroy: Some(policing_destroy),
                    resize: Some(policing_resize),
                    update_params_json: Some(policing_update_params_json),
                    tick: Some(policing_tick),
                    is_ready: Some(policing_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut PolicingEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<PolicingEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline]
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

#[inline]
fn scale_color(color: SkydimoRgb, factor: f32) -> SkydimoRgb {
    if factor <= 0.0 {
        SkydimoRgb::default()
    } else if factor >= 1.0 {
        color
    } else {
        SkydimoRgb {
            r: scale_channel(color.r, factor),
            g: scale_channel(color.g, factor),
            b: scale_channel(color.b, factor),
        }
    }
}

#[inline]
fn scale_channel(channel: u8, factor: f32) -> u8 {
    (channel as f32).mul_add(factor, 0.5).floor().clamp(0.0, 255.0) as u8
}

#[inline]
fn lerp_rgb(left: SkydimoRgb, right: SkydimoRgb, t: f32) -> SkydimoRgb {
    let inv = 1.0 - t;
    SkydimoRgb {
        r: to_u8(left.r as f32 * inv + right.r as f32 * t),
        g: to_u8(left.g as f32 * inv + right.g as f32 * t),
        b: to_u8(left.b as f32 * inv + right.b as f32 * t),
    }
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

fn parse_hex_color(raw: &[u8]) -> Option<SkydimoRgb> {
    let mut bytes = trim_ascii(raw);
    if let Some(stripped) = bytes.strip_prefix(b"#") {
        bytes = stripped;
    }

    match bytes.len() {
        3 => Some(SkydimoRgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: parse_hex_byte(bytes[0], bytes[1])?,
            g: parse_hex_byte(bytes[2], bytes[3])?,
            b: parse_hex_byte(bytes[4], bytes[5])?,
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

fn json_string<'a>(bytes: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut pos = json_value_start(bytes, key)?;
    if bytes.get(pos).copied()? != b'"' {
        return None;
    }
    pos += 1;
    let start = pos;
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some(&bytes[start..pos]);
        }
        pos += 1;
    }
    None
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

fn skip_ascii_ws(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while let Some((first, rest)) = bytes.split_first() {
        if !first.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    while let Some((last, rest)) = bytes.split_last() {
        if !last.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    bytes
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
        let mut state = nanos ^ 0xA076_1D64_78BD_642F;
        if state == 0 {
            state = 0x9E37_79B9_7F4A_7C15;
        }
        Self { state }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
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
    use std::ffi::c_void;

    use super::{
        json_bool, json_number, json_string, parse_hex_color, skydimo_plugin_get_api,
        PolicingEffect,
    };
    use crate::abi::{
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_params_without_json_allocation() {
        let json = br##"{"speed":75,"width":30,"random":true,"color":"#0af"}"##;
        assert_eq!(json_number(json, b"speed"), Some(75.0));
        assert_eq!(json_number(json, b"width"), Some(30.0));
        assert_eq!(json_bool(json, b"random"), Some(true));
        assert_eq!(json_string(json, b"color"), Some(&b"#0af"[..]));
    }

    #[test]
    fn parses_hex_colors() {
        let full = parse_hex_color(b"#FF8000").unwrap();
        assert_eq!((full.r, full.g, full.b), (255, 128, 0));

        let short = parse_hex_color(b"#0af").unwrap();
        assert_eq!((short.r, short.g, short.b), (0, 170, 255));
    }

    #[test]
    fn initial_tick_matches_lua_flash_phase() {
        let mut effect = PolicingEffect::new();
        effect.resize(8, 1, 8);
        let mut pixels = [SkydimoRgb::default(); 8];

        effect.tick(0.0, &mut pixels);

        assert!(pixels
            .iter()
            .all(|pixel| *pixel == SkydimoRgb { r: 255, g: 0, b: 0 }));
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
            assert_eq!(api.effect.resize.unwrap()(instance, 8, 1, 8), 0);

            let mut pixels = [SkydimoRgb::default(); 8];
            assert_eq!(
                api.effect
                    .tick
                    .unwrap()(instance, 0.0, pixels.as_mut_ptr(), pixels.len()),
                0
            );
            assert!(pixels
                .iter()
                .all(|pixel| *pixel == SkydimoRgb { r: 255, g: 0, b: 0 }));

            api.effect.destroy.unwrap()(instance);
        }
    }
}
