pub fn number_field(json: &str, key: &str) -> Option<f32> {
    let raw = value_slice(json, key)?;
    raw.parse::<f32>().ok()
}

pub fn bool_field(json: &str, key: &str) -> Option<bool> {
    match value_slice(json, key)? {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn string_field(json: &str, key: &str) -> Option<String> {
    let raw = value_slice(json, key)?;
    if raw.starts_with('"') {
        return unquote_json_string(raw);
    }
    Some(raw.to_string())
}

#[allow(dead_code)]
pub fn number_array_field(json: &str, key: &str, out: &mut Vec<f32>) -> bool {
    let Some(raw) = value_slice(json, key) else {
        return false;
    };
    let bytes = raw.as_bytes();
    if bytes.first().copied() != Some(b'[') {
        return false;
    }

    out.clear();
    let mut i = 1usize;
    while i < bytes.len() {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\n' | b'\r' | b'\t' | b',') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b']' {
            break;
        }

        let start = i;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
                i += 1;
            } else {
                break;
            }
        }
        if start == i {
            return false;
        }
        let Ok(value) = raw[start..i].parse::<f32>() else {
            return false;
        };
        out.push(value);
    }

    !out.is_empty()
}

pub fn string_array_field(json: &str, key: &str, out: &mut Vec<String>) -> bool {
    let Some(raw) = value_slice(json, key) else {
        return false;
    };
    let bytes = raw.as_bytes();
    if bytes.first().copied() != Some(b'[') {
        return false;
    }

    out.clear();
    let mut i = 1usize;
    while i < bytes.len() {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\n' | b'\r' | b'\t' | b',') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b']' {
            break;
        }
        if bytes[i] != b'"' {
            return false;
        }
        i += 1;

        let mut value = String::new();
        while i < bytes.len() {
            match bytes[i] {
                b'\\' if i + 1 < bytes.len() => {
                    i += 1;
                    value.push(bytes[i] as char);
                }
                b'"' => {
                    i += 1;
                    break;
                }
                ch => value.push(ch as char),
            }
            i += 1;
        }

        if !value.trim().is_empty() {
            out.push(value);
        }
    }

    true
}

fn value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = value_start(json, key)?;
    if raw.starts_with('"') {
        let end = quoted_end(raw)?;
        return Some(raw[..end].trim());
    }
    if raw.starts_with('[') {
        let end = bracket_end(raw)?;
        return Some(raw[..end].trim());
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

fn value_start<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

fn quoted_end(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut i = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

#[allow(dead_code)]
fn unquote_json_string(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    if bytes.first().copied() != Some(b'"') {
        return None;
    }

    let mut out = String::new();
    let mut i = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                i += 1;
                match bytes[i] {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    ch => out.push(ch as char),
                }
            }
            b'"' => return Some(out),
            ch => out.push(ch as char),
        }
        i += 1;
    }

    None
}

fn bracket_end(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => i += 2,
            b'"' => {
                in_string = !in_string;
                i += 1;
            }
            b'[' if !in_string => {
                depth += 1;
                i += 1;
            }
            b']' if !in_string => {
                depth = depth.saturating_sub(1);
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => i += 1,
        }
    }
    None
}
