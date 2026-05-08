use crate::abi::SkydimoRgb;

pub fn fill_black(pixels: &mut [SkydimoRgb]) {
    pixels.fill(SkydimoRgb::default());
}

pub fn white() -> SkydimoRgb {
    SkydimoRgb {
        r: 255,
        g: 255,
        b: 255,
    }
}

pub fn hex_to_rgb(hex: &str) -> SkydimoRgb {
    let raw = hex.trim().trim_start_matches('#');
    if raw.len() == 3 {
        let mut expanded = [0u8; 6];
        let bytes = raw.as_bytes();
        expanded[0] = bytes[0];
        expanded[1] = bytes[0];
        expanded[2] = bytes[1];
        expanded[3] = bytes[1];
        expanded[4] = bytes[2];
        expanded[5] = bytes[2];
        return parse_hex_rgb(&expanded).unwrap_or_else(white);
    }
    if raw.len() == 6 {
        return parse_hex_rgb(raw.as_bytes()).unwrap_or_else(white);
    }
    white()
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> SkydimoRgb {
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

pub fn rgb_to_hsv(rgb: SkydimoRgb) -> (f32, f32, f32) {
    let rf = rgb.r as f32 / 255.0;
    let gf = rgb.g as f32 / 255.0;
    let bf = rgb.b as f32 / 255.0;
    let maxc = rf.max(gf).max(bf);
    let minc = rf.min(gf).min(bf);
    let delta = maxc - minc;

    let saturation = if maxc == 0.0 { 0.0 } else { delta / maxc };
    let hue = if delta == 0.0 {
        0.0
    } else if maxc == rf {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if maxc == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    (hue.rem_euclid(360.0), saturation, maxc)
}

pub fn screen_blend_channel(a: u8, b: u8) -> u8 {
    let af = a as f32 / 255.0;
    let bf = b as f32 / 255.0;
    ((1.0 - (1.0 - af) * (1.0 - bf)) * 255.0)
        .floor()
        .clamp(0.0, 255.0) as u8
}

pub fn screen_blend(base: SkydimoRgb, over: SkydimoRgb) -> SkydimoRgb {
    if over.r == 0 && over.g == 0 && over.b == 0 {
        return base;
    }
    SkydimoRgb {
        r: screen_blend_channel(base.r, over.r),
        g: screen_blend_channel(base.g, over.g),
        b: screen_blend_channel(base.b, over.b),
    }
}

pub fn interpolate_black(color: SkydimoRgb, t: f32) -> SkydimoRgb {
    SkydimoRgb {
        r: to_u8(color.r as f32 * t),
        g: to_u8(color.g as f32 * t),
        b: to_u8(color.b as f32 * t),
    }
}

pub fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn parse_hex_rgb(raw: &[u8]) -> Option<SkydimoRgb> {
    Some(SkydimoRgb {
        r: hex_byte(raw[0], raw[1])?,
        g: hex_byte(raw[2], raw[3])?,
        b: hex_byte(raw[4], raw[5])?,
    })
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    Some((hex_nibble(high)? << 4) | hex_nibble(low)?)
}

fn hex_nibble(ch: u8) -> Option<u8> {
    match ch {
        b'0'..=b'9' => Some(ch - b'0'),
        b'a'..=b'f' => Some(ch - b'a' + 10),
        b'A'..=b'F' => Some(ch - b'A' + 10),
        _ => None,
    }
}
