//! Menu bar icon rendering.
//!
//! Drawn as raw RGBA rather than loaded from a file, because the glyph carries a live value
//! and there is no sensible set of pre-rendered files for every percentage.
//!
//! Colour discipline (blueprint 7.5): below the critical threshold the icon is a template
//! image, so macOS tints it to match the menu bar and it stays monochrome in both
//! appearances. Only above the threshold does it become a coloured image. A menu bar item
//! that is permanently lit is the reason people remove menu bar apps.

/// Above this, the glyph stops being monochrome.
pub const CRITICAL_PERCENT: f32 = 85.0;

/// `--level-critical`, the one colour the tray is ever allowed to use.
const CRITICAL_RGB: (u8, u8, u8) = (0xFF, 0x5E, 0x5B);

/// Logical size of a menu bar item. The buffer is rendered at 2x for Retina.
const LOGICAL: u32 = 16;
const SCALE: u32 = 2;

pub struct Glyph {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Whether macOS should tint this itself.
    pub template: bool,
}

/// A vertical bar whose fill height is the reported usage.
///
/// The outline is always drawn, so an empty quota still shows the item is alive.
pub fn bar(percent: Option<f32>) -> Glyph {
    let width = LOGICAL * SCALE;
    let height = LOGICAL * SCALE;
    let filled = percent.unwrap_or(0.0).clamp(0.0, 100.0);
    let critical = filled > CRITICAL_PERCENT;

    // A template image is black with an alpha mask; macOS replaces the colour.
    let (r, g, b) = if critical { CRITICAL_RGB } else { (0, 0, 0) };

    let mut rgba = vec![0u8; (width * height * 4) as usize];

    // A tall narrow bar rather than a square: a filled square reads as a solid blob next to
    // the thin-stroke icons macOS puts in the menu bar, and hides how full it is.
    let left = SCALE * 5;
    let right = width - SCALE * 5;
    let top = SCALE * 2;
    let bottom = height - SCALE * 2;
    let stroke = SCALE;
    // Clear space between the outline and the fill, so 80% and 100% are told apart at a
    // glance rather than by looking for a one-pixel gap.
    let gap = SCALE;

    let fill_left = left + stroke + gap;
    let fill_right = right - stroke - gap;
    let fill_bottom = bottom - stroke - gap;
    let fill_span = fill_bottom - (top + stroke + gap);
    let fill_top = fill_bottom - ((fill_span as f32) * filled / 100.0).round() as u32;

    for y in top..bottom {
        for x in left..right {
            let on_edge = x < left + stroke
                || x >= right - stroke
                || y < top + stroke
                || y >= bottom - stroke;
            let inside_fill = y >= fill_top
                && y < fill_bottom
                && x >= fill_left
                && x < fill_right
                && filled > 0.0;

            let alpha = if on_edge {
                // The outline stays legible against both menu bar appearances.
                0xB0
            } else if inside_fill {
                0xFF
            } else if percent.is_none() {
                // No reading yet: a faint wash rather than an empty box.
                0x30
            } else {
                0
            };

            if alpha == 0 {
                continue;
            }
            let offset = ((y * width + x) * 4) as usize;
            rgba[offset] = r;
            rgba[offset + 1] = g;
            rgba[offset + 2] = b;
            rgba[offset + 3] = alpha;
        }
    }

    Glyph {
        rgba,
        width,
        height,
        template: !critical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(glyph: &Glyph, x: u32, y: u32) -> u8 {
        glyph.rgba[((y * glyph.width + x) * 4 + 3) as usize]
    }

    fn filled_rows(glyph: &Glyph) -> usize {
        let centre = glyph.width / 2;
        (0..glyph.height)
            .filter(|y| alpha_at(glyph, centre, *y) == 0xFF)
            .count()
    }

    #[test]
    fn the_buffer_is_the_size_the_tray_expects() {
        let glyph = bar(Some(50.0));
        assert_eq!(glyph.width, 32);
        assert_eq!(glyph.height, 32);
        assert_eq!(glyph.rgba.len(), 32 * 32 * 4);
    }

    #[test]
    fn fill_height_tracks_the_reported_percentage() {
        let empty = filled_rows(&bar(Some(0.0)));
        let half = filled_rows(&bar(Some(50.0)));
        let full = filled_rows(&bar(Some(100.0)));
        assert!(half > empty, "50% must show more fill than 0%");
        assert!(full > half, "100% must show more fill than 50%");
    }

    #[test]
    fn the_glyph_stays_monochrome_until_the_quota_is_critical() {
        for percent in [0.0, 50.0, 84.9, 85.0] {
            let glyph = bar(Some(percent));
            assert!(glyph.template, "{percent}% must stay a template image");
            assert!(
                glyph
                    .rgba
                    .chunks(4)
                    .all(|px| px[0] == 0 && px[1] == 0 && px[2] == 0),
                "{percent}% must carry no colour of its own"
            );
        }

        let critical = bar(Some(85.1));
        assert!(!critical.template);
        assert!(critical
            .rgba
            .chunks(4)
            .any(|px| (px[0], px[1], px[2]) == CRITICAL_RGB));
    }

    #[test]
    fn an_unknown_reading_still_draws_an_outline() {
        let glyph = bar(None);
        assert!(
            glyph.rgba.chunks(4).any(|px| px[3] > 0),
            "the item must remain visible before the first reading"
        );
        assert_eq!(filled_rows(&glyph), 0, "nothing may be shown as used");
    }
}
