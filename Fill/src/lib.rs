mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: f32 = 50.0;
const DEFAULT_COLOR: SkydimoRgb = SkydimoRgb { r: 255, g: 0, b: 0 };

struct FillEffect {
    speed: f32,
    random_enabled: bool,
    user_color: SkydimoRgb,
    random_color: SkydimoRgb,
    time_acc: f64,
    observed_cycle: i64,
    last_elapsed: Option<f64>,
    width: usize,
    height: usize,
    rng: XorShift64,
}

impl FillEffect {
    fn new() -> Self {
        let mut rng = XorShift64::seeded();
        Self {
            speed: DEFAULT_SPEED,
            random_enabled: false,
            user_color: DEFAULT_COLOR,
            random_color: random_rgb(&mut rng),
            time_acc: 0.0,
            observed_cycle: 0,
            last_elapsed: None,
            width: 0,
            height: 1,
            rng,
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = if width == 0 {
            led_count.max(1) as usize
        } else {
            width as usize
        };
        self.height = height.max(1) as usize;
    }

    fn update_params_json(&mut self, bytes: &[u8]) {
        if let Some(speed) = json_number(bytes, b"speed") {
            if speed.is_finite() {
                self.speed = speed;
            }
        }

        if let Some(random_enabled) = json_bool(bytes, b"random") {
            self.random_enabled = random_enabled;
        }

        if let Some(color) = json_string(bytes, b"color").and_then(parse_hex_color) {
            self.user_color = color;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        self.advance_time(elapsed_seconds);
        let cycle = self.time_acc.floor() as i64;
        let progress = (self.time_acc - cycle as f64) as f32;
        self.sync_random_cycle(cycle);

        let color = if self.random_enabled {
            self.random_color
        } else {
            self.user_color
        };
        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width.max(1)
        };
        let height = self.height.max(1);
        let render_len = pixels.len().min(width.saturating_mul(height));

        if cycle.rem_euclid(2) == 1 {
            fill_rgb(&mut pixels[..render_len], scale_rgb(color, 1.0 - progress));
            if render_len < pixels.len() {
                pixels[render_len..].fill(SkydimoRgb::default());
            }
            return;
        }

        render_fill_phase(pixels, width, height, color, progress);
    }

    fn advance_time(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() {
            return;
        }

        let delta = match self.last_elapsed {
            Some(last) if elapsed_seconds >= last => elapsed_seconds - last,
            _ => 0.0,
        };
        self.last_elapsed = Some(elapsed_seconds);

        if delta > 0.0 {
            self.time_acc += 0.01 * self.speed as f64 * delta;
        }
    }

    fn sync_random_cycle(&mut self, cycle: i64) {
        if cycle == self.observed_cycle {
            return;
        }

        if self.random_enabled {
            self.random_color = random_rgb(&mut self.rng);
        }
        self.observed_cycle = cycle;
    }
}

unsafe extern "C" fn fill_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host = unsafe { &*host };
            if host.abi_version < SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(FillEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn fill_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<FillEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn fill_resize(
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

unsafe extern "C" fn fill_update_params_json(
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

unsafe extern "C" fn fill_tick(
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

unsafe extern "C" fn fill_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the native-c ABI declared in manifest.json.
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
                    create: Some(fill_create),
                    destroy: Some(fill_destroy),
                    resize: Some(fill_resize),
                    update_params_json: Some(fill_update_params_json),
                    tick: Some(fill_tick),
                    is_ready: Some(fill_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut FillEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<FillEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn render_fill_phase(
    pixels: &mut [SkydimoRgb],
    width: usize,
    height: usize,
    color: SkydimoRgb,
    progress: f32,
) {
    let render_len = pixels.len().min(width.saturating_mul(height));
    if render_len == 0 {
        return;
    }

    let first_row_len = width.min(render_len);
    let position = progress * width as f32;
    for (x, pixel) in pixels[..first_row_len].iter_mut().enumerate() {
        let distance = position - x as f32;
        *pixel = if distance > 1.0 {
            color
        } else if distance > 0.0 {
            scale_rgb(color, distance)
        } else {
            SkydimoRgb::default()
        };
    }

    let mut filled = first_row_len;
    let mut rows_filled = 1usize;
    while filled < render_len && rows_filled < height {
        let copy_len = first_row_len.min(render_len - filled);
        unsafe {
            std::ptr::copy_nonoverlapping(
                pixels.as_ptr(),
                pixels.as_mut_ptr().add(filled),
                copy_len,
            );
        }
        filled += copy_len;
        rows_filled += 1;
    }

    if render_len < pixels.len() {
        pixels[render_len..].fill(SkydimoRgb::default());
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

fn scale_rgb(color: SkydimoRgb, factor: f32) -> SkydimoRgb {
    if factor <= 0.0 {
        return SkydimoRgb::default();
    }
    if factor >= 1.0 {
        return color;
    }

    SkydimoRgb {
        r: scale_channel(color.r, factor),
        g: scale_channel(color.g, factor),
        b: scale_channel(color.b, factor),
    }
}

#[inline]
fn scale_channel(channel: u8, factor: f32) -> u8 {
    (channel as f32 * factor).floor().clamp(0.0, 255.0) as u8
}

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f32> {
    let mut pos = json_value_start(bytes, key)?;
    if bytes.get(pos).copied() == Some(b'"') {
        pos += 1;
    }

    let start = pos;
    while pos < bytes.len()
        && matches!(
            bytes[pos],
            b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'
        )
    {
        pos += 1;
    }
    if pos == start {
        return None;
    }

    std::str::from_utf8(&bytes[start..pos]).ok()?.parse().ok()
}

fn json_bool(bytes: &[u8], key: &[u8]) -> Option<bool> {
    let pos = json_value_start(bytes, key)?;
    if bytes[pos..].starts_with(b"true") {
        Some(true)
    } else if bytes[pos..].starts_with(b"false") {
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

#[inline]
fn random_rgb(rng: &mut XorShift64) -> SkydimoRgb {
    hsv_to_rgb(rng.next_f32() * 360.0, 1.0, 1.0)
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

#[inline]
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
        self.state = x.max(1);
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
        fill_rgb, json_bool, json_number, json_string, parse_hex_color, render_fill_phase,
        scale_rgb, skydimo_plugin_get_api, FillEffect,
    };
    use crate::abi::{
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_params_without_json_allocation() {
        let json = br##"{"speed":75,"random":true,"color":" #0af "}"##;
        assert_eq!(json_number(json, b"speed"), Some(75.0));
        assert_eq!(json_bool(json, b"random"), Some(true));
        assert_eq!(json_string(json, b"color"), Some(&b" #0af "[..]));
        assert_eq!(parse_hex_color(b" #0af "), Some(rgb(0, 170, 255)));
    }

    #[test]
    fn fills_buffer_with_doubling_copy() {
        let mut pixels = [SkydimoRgb::default(); 9];
        fill_rgb(&mut pixels, rgb(9, 8, 7));
        assert!(pixels.iter().all(|pixel| *pixel == rgb(9, 8, 7)));
    }

    #[test]
    fn render_fill_phase_reuses_first_row_for_matrix() {
        let mut pixels = [SkydimoRgb::default(); 8];
        render_fill_phase(&mut pixels, 4, 2, rgb(100, 50, 25), 0.125);
        assert_eq!(pixels[0], rgb(50, 25, 12));
        assert_eq!(pixels[1], SkydimoRgb::default());
        assert_eq!(pixels[..4], pixels[4..]);
    }

    #[test]
    fn fade_phase_scales_whole_buffer_like_lua_floor() {
        assert_eq!(scale_rgb(rgb(255, 128, 1), 0.75), rgb(191, 96, 0));
    }

    #[test]
    fn first_tick_matches_lua_initial_black_frame() {
        let mut effect = FillEffect::new();
        effect.resize(4, 1, 4);
        let mut pixels = [rgb(1, 2, 3); 4];
        effect.tick(0.5, &mut pixels);
        assert!(pixels.iter().all(|pixel| *pixel == SkydimoRgb::default()));

        effect.tick(0.52, &mut pixels);
        assert_eq!(pixels[0], rgb(10, 0, 0));
        assert_eq!(pixels[1], SkydimoRgb::default());
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
            assert_eq!(api.effect.resize.unwrap()(instance, 4, 1, 4), 0);
        }
        assert!(!instance.is_null());

        let mut pixels = [SkydimoRgb::default(); 4];
        unsafe {
            assert_eq!(
                api.effect
                    .tick
                    .unwrap()(instance, 0.0, pixels.as_mut_ptr(), pixels.len()),
                0
            );
            api.effect.destroy.unwrap()(instance);
        }
    }

    fn rgb(r: u8, g: u8, b: u8) -> SkydimoRgb {
        SkydimoRgb { r, g, b }
    }
}
