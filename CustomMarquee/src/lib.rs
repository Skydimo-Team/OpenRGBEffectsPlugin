mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: f64 = 25.0;
const DEFAULT_COLOR: SkydimoRgb = SkydimoRgb { r: 0, g: 0, b: 0 };
const MAX_COLORS: usize = 100;

struct CustomMarqueeEffect {
    speed: f64,
    palette: Vec<SkydimoRgb>,
    row_cache: Vec<SkydimoRgb>,
    progress: f64,
    last_elapsed: f64,
    width: usize,
    height: usize,
}

impl CustomMarqueeEffect {
    fn new() -> Self {
        Self {
            speed: DEFAULT_SPEED,
            palette: vec![DEFAULT_COLOR],
            row_cache: Vec::new(),
            progress: 0.0,
            last_elapsed: 0.0,
            width: 0,
            height: 1,
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params_json(&mut self, bytes: &[u8]) {
        if let Some(speed) = json_number(bytes, b"speed") {
            if speed.is_finite() {
                self.speed = speed;
            }
        }

        if let Some(palette) = json_palette(bytes, b"colors") {
            self.palette = palette;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            self.advance(elapsed_seconds);
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width
        }
        .max(1);
        let height = self.height.max(1);
        let total = pixels.len().min(width.saturating_mul(height));
        if total == 0 {
            self.advance(elapsed_seconds);
            return;
        }

        if self.palette.len() == 1 {
            fill_rgb(&mut pixels[..total], self.palette[0]);
        } else {
            self.rebuild_row(width);
            copy_row_pattern(&self.row_cache, &mut pixels[..total]);
        }

        self.advance(elapsed_seconds);
    }

    fn rebuild_row(&mut self, width: usize) {
        let count = self.palette.len();
        let shift = self.shift_mod(count);
        self.row_cache.resize(width, DEFAULT_COLOR);

        let palette = self.palette.as_slice();
        let row = self.row_cache.as_mut_slice();
        let mut written = 0usize;
        let mut source = shift;

        while written < width {
            let run = (count - source).min(width - written);
            row[written..written + run].copy_from_slice(&palette[source..source + run]);
            written += run;
            source = 0;
        }
    }

    fn shift_mod(&self, count: usize) -> usize {
        if count <= 1 || !self.progress.is_finite() {
            return 0;
        }
        self.progress.floor().rem_euclid(count as f64) as usize
    }

    fn advance(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() {
            return;
        }

        let dt = (elapsed_seconds - self.last_elapsed).max(0.0);
        if dt > 0.0 {
            self.progress += self.speed * dt;
        }
        self.last_elapsed = elapsed_seconds;
    }
}

unsafe extern "C" fn custom_marquee_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(CustomMarqueeEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn custom_marquee_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<CustomMarqueeEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn custom_marquee_resize(
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

unsafe extern "C" fn custom_marquee_update_params_json(
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

unsafe extern "C" fn custom_marquee_tick(
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
            effect.advance(elapsed_seconds);
            return 0;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn custom_marquee_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the native-c ABI in manifest.json.
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
                    create: Some(custom_marquee_create),
                    destroy: Some(custom_marquee_destroy),
                    resize: Some(custom_marquee_resize),
                    update_params_json: Some(custom_marquee_update_params_json),
                    tick: Some(custom_marquee_tick),
                    is_ready: Some(custom_marquee_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut CustomMarqueeEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<CustomMarqueeEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn copy_row_pattern(row: &[SkydimoRgb], pixels: &mut [SkydimoRgb]) {
    if row.is_empty() || pixels.is_empty() {
        return;
    }

    let width = row.len();
    let mut offset = 0usize;
    while offset < pixels.len() {
        let count = width.min(pixels.len() - offset);
        unsafe {
            std::ptr::copy_nonoverlapping(row.as_ptr(), pixels.as_mut_ptr().add(offset), count);
        }
        offset += count;
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

fn json_palette(bytes: &[u8], key: &[u8]) -> Option<Vec<SkydimoRgb>> {
    let mut pos = json_value_start(bytes, key)?;
    if bytes.get(pos).copied()? != b'[' {
        return None;
    }
    pos += 1;

    let mut colors = Vec::new();
    loop {
        pos = skip_ascii_ws(bytes, pos);
        match bytes.get(pos).copied() {
            Some(b']') => break,
            Some(b',') => {
                pos += 1;
                continue;
            }
            Some(b'"') => {
                let start = pos + 1;
                let end = json_string_end(bytes, start)?;
                if let Some(color) = parse_hex_color(&bytes[start..end]) {
                    colors.push(color);
                    if colors.len() >= MAX_COLORS {
                        break;
                    }
                }
                pos = end + 1;
            }
            Some(_) => {
                pos = skip_json_scalar(bytes, pos);
            }
            None => return None,
        }
    }

    if colors.is_empty() {
        colors.push(DEFAULT_COLOR);
    }
    Some(colors)
}

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f64> {
    let mut start = json_value_start(bytes, key)?;
    let quoted = bytes.get(start).copied() == Some(b'"');
    if quoted {
        start += 1;
    }

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

fn json_string_end(bytes: &[u8], mut pos: usize) -> Option<usize> {
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn skip_json_scalar(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() && !matches!(bytes[pos], b',' | b']') {
        pos += 1;
    }
    pos
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

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::{
        json_number, json_palette, parse_hex_color, skydimo_plugin_get_api, CustomMarqueeEffect,
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_speed_and_hex_palette() {
        let json = br##"{"speed":37,"colors":["#0af"," 102030 ","bad"]}"##;
        assert_eq!(json_number(json, b"speed"), Some(37.0));
        let palette = json_palette(json, b"colors").unwrap();
        assert_eq!(palette.len(), 3);
        assert_eq!(palette[0], SkydimoRgb { r: 0, g: 170, b: 255 });
        assert_eq!(palette[1], SkydimoRgb { r: 16, g: 32, b: 48 });
        assert_eq!(palette[2], SkydimoRgb { r: 187, g: 170, b: 221 });
    }

    #[test]
    fn parses_short_and_full_hex_colors() {
        assert_eq!(
            parse_hex_color(b"#abc"),
            Some(SkydimoRgb {
                r: 170,
                g: 187,
                b: 204,
            })
        );
        assert_eq!(
            parse_hex_color(b"010203"),
            Some(SkydimoRgb { r: 1, g: 2, b: 3 })
        );
    }

    #[test]
    fn renders_current_frame_before_advancing_progress() {
        let mut effect = CustomMarqueeEffect::new();
        effect.resize(4, 1, 4);
        effect.update_params_json(br##"{"speed":25,"colors":["#ff0000","#00ff00"]}"##);

        let mut pixels = [SkydimoRgb::default(); 4];
        effect.tick(1.0, &mut pixels);
        assert_eq!(
            pixels,
            [
                SkydimoRgb { r: 255, g: 0, b: 0 },
                SkydimoRgb { r: 0, g: 255, b: 0 },
                SkydimoRgb { r: 255, g: 0, b: 0 },
                SkydimoRgb { r: 0, g: 255, b: 0 },
            ]
        );

        effect.tick(1.0, &mut pixels);
        assert_eq!(
            pixels,
            [
                SkydimoRgb { r: 0, g: 255, b: 0 },
                SkydimoRgb { r: 255, g: 0, b: 0 },
                SkydimoRgb { r: 0, g: 255, b: 0 },
                SkydimoRgb { r: 255, g: 0, b: 0 },
            ]
        );
    }

    #[test]
    fn repeats_the_same_marquee_row_for_matrices() {
        let mut effect = CustomMarqueeEffect::new();
        effect.resize(3, 2, 6);
        effect.update_params_json(
            br##"{"colors":["#ff0000","#00ff00","#0000ff"],"speed":1}"##,
        );

        let mut pixels = [SkydimoRgb::default(); 6];
        effect.tick(0.0, &mut pixels);
        assert_eq!(pixels[0], pixels[3]);
        assert_eq!(pixels[1], pixels[4]);
        assert_eq!(pixels[2], pixels[5]);
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
    fn ffi_create_update_tick_destroy_round_trip() {
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
            let params = br##"{"speed":10,"colors":["#010203","#040506"]}"##;
            assert_eq!(
                api.effect.update_params_json.unwrap()(
                    instance,
                    params.as_ptr().cast::<std::ffi::c_char>(),
                    params.len(),
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
            assert_eq!(pixels[0], SkydimoRgb { r: 1, g: 2, b: 3 });
            assert_eq!(pixels[1], SkydimoRgb { r: 4, g: 5, b: 6 });

            api.effect.destroy.unwrap()(instance);
        }
    }
}
