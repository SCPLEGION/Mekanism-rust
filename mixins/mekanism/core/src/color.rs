// `mekanism.common.lib.Color` as the colour picker window (`GuiColorWindow`) uses it: HSV and RGB, and the text
// field's "r,g,b" / "r,g,b,a" form.

/// `Color.hsv(h, s, v)`: `h` in degrees, `s` and `v` from 0 to 1; the channels rounded to bytes.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let (h, s, v) = (f64::from(h).rem_euclid(360.0), f64::from(s).clamp(0.0, 1.0), f64::from(v).clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|ch| ((ch + m) * 255.0).round() as u8)
}

/// `Color.hsvArray`: hue in degrees, saturation and value from 0 to 1.
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0);
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    (hue as f32, if max == 0.0 { 0.0 } else { (delta / max) as f32 }, max as f32)
}

pub fn argb(rgb: [u8; 3], alpha: u8) -> u32 {
    u32::from(alpha) << 24 | u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2])
}

/// The field's text for a colour: "r,g,b", with ",a" when the window handles alpha.
pub fn text(argb: u32, alpha: bool) -> String {
    let (a, r, g, b) = (argb >> 24, argb >> 16 & 0xFF, argb >> 8 & 0xFF, argb & 0xFF);
    if alpha { format!("{r},{g},{b},{a}") } else { format!("{r},{g},{b}") }
}

/// `updateColorFromText`: the colour of a complete, in-range "r,g,b[,a]" (ARGB; opaque without alpha).
pub fn parse(text: &str, alpha: bool) -> Option<u32> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != if alpha { 4 } else { 3 } {
        return None;
    }
    let mut v = [0xFF_u32; 4];
    for (i, p) in parts.iter().enumerate() {
        v[i] = p.parse::<u32>().ok().filter(|n| *n <= 0xFF)?;
    }
    Some(v[3] << 24 | v[0] << 16 | v[1] << 8 | v[2])
}

/// `InputValidator.DIGIT.or(c -> c == ',')`.
pub fn allowed(c: char) -> bool {
    c.is_ascii_digit() || c == ','
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_the_primaries() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), [0, 0, 255]);
        assert_eq!(hsv_to_rgb(0.0, 0.0, 0.5), [128, 128, 128]);
        let (h, s, v) = rgb_to_hsv(0, 128, 255);
        assert_eq!(hsv_to_rgb(h, s, v), [0, 128, 255]);
    }

    #[test]
    fn the_field_text_is_read_and_written() {
        assert_eq!(text(0x80FF_8000, true), "255,128,0,128");
        assert_eq!(text(0x80FF_8000, false), "255,128,0");
        assert_eq!(parse("255,128,0,128", true), Some(0x80FF_8000));
        assert_eq!(parse("255,128,0", false), Some(0xFFFF_8000));
        assert_eq!(parse("256,0,0", false), None);
        assert_eq!(parse("1,2", false), None);
        assert!(allowed('7') && allowed(',') && !allowed(' '));
    }
}
