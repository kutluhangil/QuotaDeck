//! Menu bar icon rendering.
//!
//! Drawn as raw RGBA rather than loaded from a file, because the glyph carries a live value
//! and there is no sensible set of pre-rendered files for every percentage.
//!
//! Colour discipline (blueprint 7.5): below the critical threshold the icon is a template
//! image, so macOS tints it to match the menu bar and it stays monochrome in both
//! appearances. Only above the threshold does it become a coloured image. A menu bar item
//! that is permanently lit is the reason people remove menu bar apps.
//!
//! Only macOS has template images. A black glyph is invisible against the dark Windows taskbar
//! everyone runs by default, and against the dark panel most Linux desktops ship, so the
//! monochrome ink is a mid grey that holds against either. Reading `SystemUsesLightTheme` out
//! of the registry would be exact on Windows, and it would cost a Windows API dependency to
//! move a glyph that is sixteen pixels tall between two greys — [`MONOCHROME_RGB`] is the
//! honest trade, and Linux has no equivalent to read in the first place.

/// Above this, the glyph stops being monochrome.
pub const CRITICAL_PERCENT: f32 = 85.0;

/// `--level-critical`, the one colour the tray is ever allowed to use.
const CRITICAL_RGB: (u8, u8, u8) = (0xFF, 0x5E, 0x5B);

/// The ink below the critical threshold.
///
/// Black on macOS, where it is a template image and the system replaces the colour outright.
/// A readable grey everywhere else, where nothing replaces anything.
const MONOCHROME_RGB: (u8, u8, u8) = if cfg!(target_os = "macos") {
    (0, 0, 0)
} else {
    (0x9A, 0x9A, 0x9A)
};

/// Whether the system tints the glyph for us. Only macOS does.
const TEMPLATE_CAPABLE: bool = cfg!(target_os = "macos");

/// Logical size of a menu bar item. The buffer is rendered at 2x for Retina.
const LOGICAL: u32 = 16;
const SCALE: u32 = 2;

/// Logical width of the strip mode. Wide enough to read as a timeline, narrow enough that the
/// item is not the widest thing in the menu bar.
const STRIP_LOGICAL_WIDTH: u32 = 44;
/// Columns the tray asks [`quotadeck_core::horizon::columns`] for.
///
/// Each column is 2 logical pixels wide with 1 between them, which is the finest grain that
/// still separates on a non-Retina display.
pub const STRIP_COLUMNS: usize = 14;

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
    let (r, g, b) = if critical {
        CRITICAL_RGB
    } else {
        MONOCHROME_RGB
    };

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
        template: TEMPLATE_CAPABLE && !critical,
    }
}

/// Oldest and newest column opacity in the strip.
///
/// A template image can only vary alpha, so the recency ramp the panel draws with a gradient
/// mask is carried here by opacity alone — which is the same statement either way: the left of
/// the strip is the oldest work still being counted, and it should not compete with now.
const STRIP_OLDEST_ALPHA: u8 = 0x55;
const STRIP_NEWEST_ALPHA: u8 = 0xFF;
/// The horizon line itself, drawn whether or not there is any usage above it, so an idle
/// item still shows the app is running.
const HORIZON_ALPHA: u8 = 0x66;

/// A miniature of the panel's Horizon strip: usage resting on a horizon line, oldest at the
/// left, now at the right.
///
/// `columns` comes from [`quotadeck_core::horizon::columns`], so the tray and the panel fold
/// the same series by the same rule. Missing entries are drawn as empty rather than as an
/// error: a provider with no history still gets a horizon.
pub fn strip(columns: &[f32], percent: Option<f32>) -> Glyph {
    let width = STRIP_LOGICAL_WIDTH * SCALE;
    let height = LOGICAL * SCALE;
    let critical = percent.unwrap_or(0.0) > CRITICAL_PERCENT;
    let (r, g, b) = if critical {
        CRITICAL_RGB
    } else {
        MONOCHROME_RGB
    };

    let mut rgba = vec![0u8; (width * height * 4) as usize];

    let pitch = SCALE * 3;
    let bar_width = SCALE * 2;
    let ink = pitch * STRIP_COLUMNS as u32 - (pitch - bar_width);
    let left = (width - ink) / 2;

    // Bars stand on the horizon line, and never reach the very top: a column touching the
    // menu bar's edge reads as clipped rather than as full.
    let horizon_top = height - SCALE;
    let ceiling = SCALE * 2;
    let span = horizon_top - ceiling;

    let mut plot = |x: u32, y: u32, alpha: u8| {
        let offset = ((y * width + x) * 4) as usize;
        rgba[offset] = r;
        rgba[offset + 1] = g;
        rgba[offset + 2] = b;
        rgba[offset + 3] = alpha;
    };

    for x in left..left + ink {
        for y in horizon_top..height {
            plot(x, y, HORIZON_ALPHA);
        }
    }

    for index in 0..STRIP_COLUMNS {
        let value = columns.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        if value <= 0.0 {
            continue;
        }
        let bar_height = ((span as f32) * value).round().max(1.0) as u32;
        let alpha = ramp(index);
        let x0 = left + index as u32 * pitch;
        for x in x0..x0 + bar_width {
            for y in horizon_top - bar_height..horizon_top {
                plot(x, y, alpha);
            }
        }
    }

    Glyph {
        rgba,
        width,
        height,
        template: TEMPLATE_CAPABLE && !critical,
    }
}

/// Opacity for column `index`, oldest to newest.
fn ramp(index: usize) -> u8 {
    if STRIP_COLUMNS <= 1 {
        return STRIP_NEWEST_ALPHA;
    }
    let t = index as f32 / (STRIP_COLUMNS - 1) as f32;
    let span = f32::from(STRIP_NEWEST_ALPHA - STRIP_OLDEST_ALPHA);
    STRIP_OLDEST_ALPHA + (span * t).round() as u8
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
            assert_eq!(
                glyph.template, TEMPLATE_CAPABLE,
                "{percent}% must be tinted by the system wherever the system can"
            );
            // Grey is not a colour the app chose to mean something; the level ramp is. The
            // invariant is that the ramp has not reached the menu bar yet.
            let (r, g, b) = MONOCHROME_RGB;
            assert!(
                glyph
                    .rgba
                    .chunks(4)
                    .filter(|px| px[3] > 0)
                    .all(|px| px[0] == r && px[1] == g && px[2] == b),
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

    /// Tallest opaque run of pixels in the column that holds strip index `index`.
    fn column_height(glyph: &Glyph, index: usize) -> u32 {
        let pitch = SCALE * 3;
        let bar_width = SCALE * 2;
        let ink = pitch * STRIP_COLUMNS as u32 - (pitch - bar_width);
        let left = (glyph.width - ink) / 2;
        let x = left + index as u32 * pitch;
        // The horizon line runs under every column and is not part of the bar.
        (0..glyph.height - SCALE)
            .filter(|y| alpha_at(glyph, x, *y) > 0)
            .count() as u32
    }

    #[test]
    fn the_strip_is_wider_than_it_is_tall() {
        let glyph = strip(&[], None);
        assert_eq!(glyph.width, 88);
        assert_eq!(glyph.height, 32);
        assert_eq!(glyph.rgba.len(), 88 * 32 * 4);
    }

    #[test]
    fn an_empty_strip_still_draws_its_horizon() {
        let glyph = strip(&[], None);
        assert!(
            glyph.rgba.chunks(4).any(|px| px[3] > 0),
            "an idle tool must still show the app is running"
        );
        for index in 0..STRIP_COLUMNS {
            assert_eq!(column_height(&glyph, index), 0);
        }
    }

    #[test]
    fn column_height_tracks_the_folded_value() {
        let values: Vec<f32> = (0..STRIP_COLUMNS)
            .map(|i| i as f32 / (STRIP_COLUMNS - 1) as f32)
            .collect();
        let glyph = strip(&values, None);

        assert_eq!(column_height(&glyph, 0), 0, "a zero column is left empty");
        assert!(
            column_height(&glyph, STRIP_COLUMNS - 1) > column_height(&glyph, STRIP_COLUMNS / 2)
        );
    }

    #[test]
    fn a_short_series_draws_what_it_has_instead_of_failing() {
        let glyph = strip(&[1.0], None);
        assert!(column_height(&glyph, 0) > 0);
        assert_eq!(column_height(&glyph, STRIP_COLUMNS - 1), 0);
    }

    #[test]
    fn recent_columns_are_drawn_more_strongly_than_old_ones() {
        let values = vec![1.0f32; STRIP_COLUMNS];
        let glyph = strip(&values, None);
        let pitch = SCALE * 3;
        let ink = pitch * STRIP_COLUMNS as u32 - pitch + SCALE * 2;
        let left = (glyph.width - ink) / 2;
        let y = glyph.height - SCALE - 1;

        let oldest = alpha_at(&glyph, left, y);
        let newest = alpha_at(&glyph, left + (STRIP_COLUMNS as u32 - 1) * pitch, y);
        assert!(
            newest > oldest,
            "now must read louder than the capacity about to return ({newest} vs {oldest})"
        );
        assert_eq!(newest, STRIP_NEWEST_ALPHA);
    }

    #[test]
    fn the_strip_obeys_the_same_colour_rule_as_the_glyph() {
        let values = vec![1.0f32; STRIP_COLUMNS];
        for percent in [0.0, 50.0, 85.0] {
            let glyph = strip(&values, Some(percent));
            assert_eq!(
                glyph.template, TEMPLATE_CAPABLE,
                "{percent}% must be tinted by the system wherever the system can"
            );
            // Grey is not a colour the app chose to mean something; the level ramp is. The
            // invariant is that the ramp has not reached the menu bar yet.
            let (r, g, b) = MONOCHROME_RGB;
            assert!(
                glyph
                    .rgba
                    .chunks(4)
                    .filter(|px| px[3] > 0)
                    .all(|px| px[0] == r && px[1] == g && px[2] == b),
                "{percent}% must carry no colour of its own"
            );
        }

        let critical = strip(&values, Some(85.1));
        assert!(!critical.template);
        assert!(critical
            .rgba
            .chunks(4)
            .any(|px| (px[0], px[1], px[2]) == CRITICAL_RGB));
    }

    #[test]
    fn a_value_too_small_to_round_up_is_still_drawn() {
        let mut values = vec![0.0f32; STRIP_COLUMNS];
        values[3] = 0.001;
        let glyph = strip(&values, None);
        assert!(
            column_height(&glyph, 3) >= 1,
            "a gap has to mean nothing happened"
        );
    }
}
