use eframe::egui::Color32;

pub const DEFAULT_HUE: f32 = 163.0;
pub const SWATCHES: [f32; 8] = [25.0, 55.0, 90.0, 140.0, 163.0, 230.0, 265.0, 320.0];

fn oklch_to_linear(l: f32, c: f32, h: f32) -> Option<[f32; 3]> {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let rgb = [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
        -0.004_196_086 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ];
    rgb.iter().all(|v| (-1e-4..=1.0001).contains(v)).then(|| rgb.map(|v| v.clamp(0.0, 1.0)))
}

fn encode(v: f32) -> u8 {
    let s = if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

#[cfg(test)]
fn decode(v: u8) -> f32 {
    let s = v as f32 / 255.0;
    if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

pub fn oklch(l: f32, c: f32, h: f32) -> Color32 {
    let mut c = c;
    loop {
        if let Some([r, g, b]) = oklch_to_linear(l, c, h) {
            return Color32::from_rgb(encode(r), encode(g), encode(b));
        }
        c *= 0.9;
        if c < 1e-3 {
            c = 0.0;
        }
    }
}

#[cfg(test)]
fn luminance(c: Color32) -> f32 {
    0.2126 * decode(c.r()) + 0.7152 * decode(c.g()) + 0.0722 * decode(c.b())
}

#[cfg(test)]
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

pub fn parse_hue(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok().filter(|v| v.is_finite()).map(|v| v.rem_euclid(360.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_extremes() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn oklch_gray_and_gamut() {
        let g = oklch(0.6, 0.0, 0.0);
        assert_eq!(g.r(), g.g());
        assert_eq!(g.g(), g.b());
        let c = oklch(0.7, 0.4, 145.0);
        assert!(c.g() > c.r() && c.g() > c.b());
    }

    #[test]
    fn parse_hue_wraps_and_rejects_garbage() {
        assert_eq!(parse_hue("163"), Some(163.0));
        assert_eq!(parse_hue("-40"), Some(320.0));
        assert_eq!(parse_hue("400"), Some(40.0));
        assert_eq!(parse_hue("abc"), None);
        assert_eq!(parse_hue("inf"), None);
    }
}
