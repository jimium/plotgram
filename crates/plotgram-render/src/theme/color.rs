//! Hex color utilities for theme synthesis (group nest ladder).

/// Blend toward white. `amount=0` returns the original color; `amount=1` returns white.
pub fn lighten(hex: &str, amount: f64) -> String {
    let amount = amount.clamp(0.0, 1.0);
    let Some((r, g, b)) = parse_hex_rgb(hex) else {
        return hex.to_string();
    };
    let nr = r as f64 + (255.0 - r as f64) * amount;
    let ng = g as f64 + (255.0 - g as f64) * amount;
    let nb = b as f64 + (255.0 - b as f64) * amount;
    to_hex_rgb(nr.round() as u8, ng.round() as u8, nb.round() as u8)
}

/// Blend toward black. `amount=0` returns the original color; `amount=1` returns black.
pub fn darken(hex: &str, amount: f64) -> String {
    let amount = amount.clamp(0.0, 1.0);
    let Some((r, g, b)) = parse_hex_rgb(hex) else {
        return hex.to_string();
    };
    let scale = 1.0 - amount;
    to_hex_rgb(
        (r as f64 * scale).round() as u8,
        (g as f64 * scale).round() as u8,
        (b as f64 * scale).round() as u8,
    )
}

fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#')?;
    match hex.len() {
        6 | 8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        3 | 4 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn to_hex_rgb(r: u8, g: u8, b: u8) -> String {
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighten_clamps_and_preserves_base() {
        assert_eq!(lighten("#ECECEF", 0.0), "#ececef");
        assert_eq!(lighten("#ECECEF", 1.0), "#ffffff");
        assert_ne!(lighten("#ECECEF", 0.35), "#ececef");
    }

    #[test]
    fn darken_steps_toward_black() {
        assert_eq!(darken("#ECECEF", 0.0), "#ececef");
        assert_eq!(darken("#ECECEF", 1.0), "#000000");
        assert_ne!(darken("#ECECEF", 0.04), "#ececef");
    }
}
