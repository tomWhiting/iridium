use iridium_config::test_support::classic_light_faces;
use iridium_editor::theme::Theme;

use super::content::{PanelAnchor, PanelCaret, PanelContent, PanelFit, PanelRow, Span};
use super::geometry::{GridMetrics, fit_for, panel_geometry, sidebar_fit_for};
use super::metrics::{
    EXPLORER_MAX_VISIBLE_ROWS, PAD_X, PAD_Y, PANEL_MAX_VISIBLE_ROWS, SIDEBAR_MAX_FRACTION,
    TOP_ANCHOR_FRACTION,
};
use super::paint::{hairline_color, panel_background, panel_spans, scroll_for, strip_background};
use crate::tab_strip::tab_strip_height;
use crate::units::index_to_f32;
use iridium_editor::theme::Color;

/// The working grid of a 14 px face on a 2× display.
const CHAR_WIDTH: f32 = 16.8;
const LINE_HEIGHT: f32 = 39.2;
const SCALE: f32 = 2.0;
const WINDOW: (f32, f32) = (3024.0, 1964.0);

/// The standard test window's grid.
const METRICS: GridMetrics = GridMetrics {
    char_width: CHAR_WIDTH,
    line_height: LINE_HEIGHT,
    scale: SCALE,
};

/// A grid whose cell tracks the given scale, as a real rescale would.
fn metrics_at(scale: f32) -> GridMetrics {
    GridMetrics {
        char_width: CHAR_WIDTH / SCALE * scale,
        line_height: LINE_HEIGHT / SCALE * scale,
        scale,
    }
}

/// A mid-sized panel placed on the standard test window.
fn placed(anchor: PanelAnchor, rows: usize, columns: usize, reserve: f32) -> super::PanelGeometry {
    panel_geometry(WINDOW.0, WINDOW.1, METRICS, anchor, rows, columns, reserve)
        .expect("the standard window holds a mid-sized panel")
}

#[test]
fn a_short_line_does_not_scroll() {
    assert_eq!(scroll_for(0, 40), 0);
    assert_eq!(scroll_for(39, 40), 0);
}

#[test]
fn the_caret_is_kept_in_the_last_cell() {
    assert_eq!(scroll_for(40, 40), 1);
    assert_eq!(scroll_for(55, 40), 16);
}

#[test]
fn a_zero_width_strip_is_not_divided_by() {
    assert_eq!(scroll_for(10, 0), 0);
}

#[test]
fn a_panel_is_horizontally_centred() {
    let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
    let left = geometry.exterior.x;
    let right = WINDOW.0 - (geometry.exterior.x + geometry.exterior.width);
    assert!((left - right).abs() < 0.001);
}

#[test]
fn the_top_anchor_is_the_web_demos_twelve_vh() {
    let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
    assert!(
        TOP_ANCHOR_FRACTION
            .mul_add(-WINDOW.1, geometry.exterior.y)
            .abs()
            < 0.001
    );
}

#[test]
fn a_bottom_panel_stacks_above_the_reserve() {
    let reserve = 60.0;
    let geometry = placed(PanelAnchor::Bottom, 2, 40, reserve);
    let bottom = geometry.exterior.y + geometry.exterior.height;
    assert!((bottom - (WINDOW.1 - reserve)).abs() < 0.001);
}

#[test]
fn the_padding_arithmetic_matches_the_web_scale() {
    let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
    let expected_width = 40.0_f32.mul_add(CHAR_WIDTH, 2.0 * PAD_X * SCALE);
    let expected_height = 8.0_f32.mul_add(LINE_HEIGHT, 2.0 * PAD_Y * SCALE);
    assert!((geometry.exterior.width - expected_width).abs() < 0.001);
    assert!((geometry.exterior.height - expected_height).abs() < 0.001);
    let expected_content_x = PAD_X.mul_add(SCALE, geometry.exterior.x);
    let expected_content_y = PAD_Y.mul_add(SCALE, geometry.exterior.y);
    assert!((geometry.content_x - expected_content_x).abs() < 0.001);
    assert!((geometry.content_y - expected_content_y).abs() < 0.001);
}

#[test]
fn the_radius_never_exceeds_half_the_short_side() {
    for rows in [1_usize, 2, 8, 12] {
        for columns in [16_usize, 40, 60] {
            let geometry = placed(PanelAnchor::Top, rows, columns, 0.0);
            let short = geometry.exterior.width.min(geometry.exterior.height);
            assert!(geometry.exterior.radius <= short / 2.0 + 0.001);
        }
    }
}

/// The standing rule the old glyph tests asserted with arc characters,
/// translated: every drawable panel keeps genuine arcs.
#[test]
fn every_drawable_panel_keeps_a_genuine_arc() {
    for (width, height, scale) in [
        (3024.0, 1964.0, 2.0),
        (1512.0, 982.0, 1.0),
        (800.0, 500.0, 1.25),
    ] {
        for anchor in [PanelAnchor::Top, PanelAnchor::Bottom] {
            if let Some(geometry) =
                panel_geometry(width, height, metrics_at(scale), anchor, 2, 20, 0.0)
            {
                assert!(geometry.exterior.radius > 0.0, "a panel corner went sharp");
            }
        }
    }
}

/// A menu-sized panel hung from a point on the standard test window.
fn hung(x: f32, y: f32) -> super::PanelGeometry {
    panel_geometry(
        WINDOW.0,
        WINDOW.1,
        METRICS,
        PanelAnchor::Point { x, y },
        7,
        20,
        0.0,
    )
    .expect("the standard window holds a menu-sized panel")
}

#[test]
fn a_point_anchored_panel_hangs_from_the_click() {
    let geometry = hung(400.0, 500.0);
    assert!((geometry.exterior.x - 400.0).abs() < 0.001);
    assert!((geometry.exterior.y - 500.0).abs() < 0.001);
}

#[test]
fn a_point_anchored_panel_is_clamped_inside_every_edge() {
    for (x, y) in [
        (WINDOW.0 - 4.0, WINDOW.1 - 4.0),
        (WINDOW.0 + 500.0, 0.0),
        (0.0, WINDOW.1 - 1.0),
        (-40.0, -40.0),
    ] {
        let geometry = hung(x, y);
        assert!(geometry.exterior.x >= -0.001, "clipped the left edge");
        assert!(geometry.exterior.y >= -0.001, "clipped the top edge");
        assert!(
            geometry.exterior.x + geometry.exterior.width <= WINDOW.0 + 0.001,
            "clipped the right edge"
        );
        assert!(
            geometry.exterior.y + geometry.exterior.height <= WINDOW.1 + 0.001,
            "clipped the bottom edge"
        );
        assert!(geometry.exterior.radius > 0.0, "a menu corner went sharp");
    }
}

#[test]
fn a_point_anchored_panel_flips_above_a_click_near_the_bottom() {
    // The macOS behaviour: a menu with no room below opens upward from the
    // click rather than sliding until it covers it.
    let click = WINDOW.1 - 20.0;
    let geometry = hung(400.0, click);
    assert!(
        (geometry.exterior.y + geometry.exterior.height - click).abs() < 0.001,
        "the panel's bottom edge sits on the click"
    );
}

#[test]
fn a_point_anchor_with_room_neither_way_slides_instead_of_flipping() {
    let height = 400.0;
    let geometry = panel_geometry(
        WINDOW.0,
        height,
        METRICS,
        PanelAnchor::Point { x: 0.0, y: 300.0 },
        7,
        20,
        0.0,
    )
    .expect("a short window still holds a seven-row menu");
    assert!(geometry.exterior.y >= -0.001);
    assert!(geometry.exterior.y + geometry.exterior.height <= height + 0.001);
}

#[test]
fn a_point_anchored_panel_declines_a_window_it_cannot_fit() {
    assert!(
        panel_geometry(
            200.0,
            200.0,
            METRICS,
            PanelAnchor::Point { x: 10.0, y: 10.0 },
            7,
            20,
            0.0,
        )
        .is_none()
    );
}

#[test]
fn the_hit_test_names_the_row_under_a_pixel_and_nothing_off_the_panel() {
    let geometry = hung(400.0, 500.0);
    let x = geometry.content_x + 1.0;
    for row in 0..7_usize {
        let y = (index_to_f32(row) + 0.5).mul_add(LINE_HEIGHT, geometry.content_y);
        assert_eq!(geometry.row_at(x, y, 7), Some(row));
    }
    assert!(
        geometry.row_at(x, geometry.exterior.y + 1.0, 7).is_none(),
        "the top padding is not row zero"
    );
    assert!(
        geometry
            .row_at(x, geometry.exterior.y + geometry.exterior.height - 1.0, 7)
            .is_none(),
        "the bottom padding is not the last row"
    );
    assert!(
        geometry
            .row_at(geometry.exterior.x - 1.0, 500.0, 7)
            .is_none(),
        "a pixel left of the panel is on no row"
    );
    assert!(!geometry.contains(geometry.exterior.x - 1.0, 500.0));
    assert!(geometry.contains(geometry.exterior.x + 1.0, geometry.exterior.y + 1.0));
}

#[test]
fn separator_rules_stay_inside_the_panel() {
    let geometry = hung(400.0, 500.0);
    for index in 0..7_usize {
        let rule = geometry.separator_rect(index);
        let band = geometry.selected_row_rect(index);
        assert!(rule.x >= geometry.exterior.x);
        assert!(rule.x + rule.width <= geometry.exterior.x + geometry.exterior.width + 0.001);
        assert!(rule.y >= band.y, "the rule sits inside its own row");
        assert!(rule.y + rule.height <= band.y + band.height + 0.001);
    }
}

#[test]
fn the_caret_rect_stays_inside_the_content_area() {
    let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
    for (row, column) in [(0_usize, 0_usize), (7, 40)] {
        let bar = geometry.caret_rect(PanelCaret { row, column });
        assert!(bar.x >= geometry.content_x - 0.001);
        let content_right = 40.0_f32.mul_add(CHAR_WIDTH, geometry.content_x);
        assert!(bar.x <= content_right + 0.001);
        assert!(bar.y >= geometry.content_y - 0.001);
        let content_bottom = 8.0_f32.mul_add(LINE_HEIGHT, geometry.content_y);
        assert!(bar.y + bar.height <= content_bottom + 0.001);
    }
}

#[test]
fn selected_row_bands_stay_inside_the_panel() {
    let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
    for index in 0..8_usize {
        let band = geometry.selected_row_rect(index);
        assert!(band.x >= geometry.exterior.x);
        assert!(band.x + band.width <= geometry.exterior.x + geometry.exterior.width + 0.001);
        assert!(band.y >= geometry.exterior.y);
        assert!(band.y + band.height <= geometry.exterior.y + geometry.exterior.height + 0.001);
        assert!(band.radius > 0.0, "a chrome corner went sharp");
    }
}

/// Content composed against a fit always produces geometry that fits the
/// window that produced the fit, at both common scales.
#[test]
fn the_fit_round_trip_fits_the_window_at_both_scales() {
    for (width, height, scale) in [(3024.0, 1964.0, 2.0), (1512.0, 982.0, 1.0)] {
        let metrics = metrics_at(scale);
        let fit = fit_for(width, height, metrics).expect("a full window fits a panel");
        for anchor in [PanelAnchor::Top, PanelAnchor::Bottom] {
            for rows in [1, fit.max_interior_rows.min(PANEL_MAX_VISIBLE_ROWS)] {
                let geometry = panel_geometry(
                    width,
                    height,
                    metrics,
                    anchor,
                    rows,
                    fit.content_columns,
                    0.0,
                )
                .expect("fit-sized content places");
                assert!(geometry.exterior.x >= 0.0);
                assert!(geometry.exterior.y >= 0.0);
                assert!(geometry.exterior.x + geometry.exterior.width <= width + 0.001);
                assert!(geometry.exterior.y + geometry.exterior.height <= height + 0.001);
            }
        }
    }
}

#[test]
fn a_window_too_small_composes_no_panel() {
    assert!(fit_for(200.0, 300.0, METRICS).is_none());
    assert!(fit_for(3024.0, 80.0, METRICS).is_none());
    let unmeasured = GridMetrics {
        char_width: 0.0,
        ..METRICS
    };
    assert!(fit_for(3024.0, 1964.0, unmeasured).is_none());
}

/// The whole point of the placement, stated as the one number that
/// separates the two fits: a sidebar fills its band, a popover does not
/// fill the window.
#[test]
fn a_sidebar_browses_further_down_than_a_popover_ever_will() {
    let (width, height) = WINDOW;
    let strip = tab_strip_height(height, METRICS);
    let popover = fit_for(width, height, METRICS).expect("the test window fits a popover");
    let sidebar = sidebar_fit_for(width, height, strip, METRICS).expect("and it fits a sidebar");

    assert_eq!(
        popover.max_browse_rows, EXPLORER_MAX_VISIBLE_ROWS,
        "the popover is capped by taste on this window, which is what makes the \
         comparison below mean anything"
    );
    assert_eq!(
        sidebar.max_browse_rows,
        sidebar.max_interior_rows - 1,
        "a sidebar takes every row of its band but the query row"
    );
    assert!(
        sidebar.max_browse_rows > popover.max_browse_rows,
        "sidebar {} is no taller than popover {}",
        sidebar.max_browse_rows,
        popover.max_browse_rows
    );
}

/// The other half: it is a *column*, so it gives the document back the
/// width a popover takes from the middle of the window.
#[test]
fn a_sidebar_is_narrower_than_a_popover_and_never_takes_half_the_window() {
    let (width, height) = WINDOW;
    let popover = fit_for(width, height, METRICS).expect("a popover fits");
    let sidebar = sidebar_fit_for(width, height, 0.0, METRICS).expect("a sidebar fits");
    assert!(sidebar.content_columns < popover.content_columns);

    // Half of a window one third the standard width: the fraction binds
    // here where `SIDEBAR_COLUMNS` binds above, and the two rules are what
    // keep a sidebar honest on both ends of the range.
    let narrow = 1000.0;
    let squeezed = sidebar_fit_for(narrow, height, 0.0, METRICS).expect("a sidebar still fits");
    let exterior = index_to_f32(squeezed.content_columns)
        .mul_add(METRICS.char_width, 2.0 * PAD_X * METRICS.scale);
    assert!(
        exterior <= SIDEBAR_MAX_FRACTION * narrow,
        "{exterior} of {narrow} is more than the sidebar's share"
    );
}

/// A window that cannot hold a readable column says so, rather than
/// drawing a sliver — the caller falls back to the popover.
#[test]
fn a_window_that_cannot_hold_a_column_refuses_one() {
    let (width, height) = WINDOW;
    assert!(sidebar_fit_for(600.0, height, 0.0, METRICS).is_none());
    assert!(sidebar_fit_for(width, 80.0, 0.0, METRICS).is_none());
    // ⚠️ **A sidebar survives a shorter window than a popover does, and
    // that is the rule rather than an oversight.** `fit_for` gives up 12%
    // of the height to the top anchor before it measures; a column hangs
    // off the top edge and has no anchor to pay for. On this grid a row
    // costs 39.2 px over 48 px of padding, so the popover needs 99.1 px of
    // window and the sidebar 87.2 — and 92 is the gap between them.
    // MEASURED: the first pair of numbers written here were both above
    // *both* thresholds and proved nothing.
    assert!(sidebar_fit_for(width, 92.0, 0.0, METRICS).is_some());
    assert!(fit_for(width, 92.0, METRICS).is_none());
    // A reserve taller than the window, and one that never measured.
    assert!(sidebar_fit_for(width, height, height + 1.0, METRICS).is_none());
    assert!(sidebar_fit_for(width, height, f32::NAN, METRICS).is_none());
    let unmeasured = GridMetrics {
        char_width: 0.0,
        ..METRICS
    };
    assert!(sidebar_fit_for(width, height, 0.0, unmeasured).is_none());
}

/// A sidebar's band clears the strip: its rows plus the strip fit the
/// window, which is the arithmetic the reserve exists to make true.
#[test]
fn a_sidebar_composed_against_its_fit_clears_the_tab_strip() {
    for (width, height, scale) in [(3024.0, 1964.0, 2.0), (1512.0, 982.0, 1.0)] {
        let metrics = metrics_at(scale);
        let strip = tab_strip_height(height, metrics);
        let fit = sidebar_fit_for(width, height, strip, metrics).expect("a sidebar fits");
        let tall = index_to_f32(fit.max_interior_rows)
            .mul_add(metrics.line_height, 2.0 * PAD_Y * metrics.scale);
        assert!(
            strip + tall <= height + 0.001,
            "a {scale}× window's sidebar overlaps its tab strip: {strip} + {tall} > {height}"
        );
    }
}

/// One row of band leaves one row of list, never zero — a panel that
/// composed no rows at all would be an empty box.
#[test]
fn the_shortest_honest_band_still_lists_a_row() {
    assert_eq!(PanelFit::sidebar(30, 1).max_browse_rows, 1);
    assert_eq!(PanelFit::popover(30, 1).max_browse_rows, 1);
}

#[test]
fn a_row_wider_than_the_panel_is_truncated_on_a_character_boundary() {
    let white = Color::new(1.0, 1.0, 1.0, 1.0);
    let panel = PanelContent {
        anchor: PanelAnchor::Top,
        content_columns: 4,
        rows: vec![PanelRow::new(vec![Span::new("héllo!", white)])],
        caret: None,
    };
    let text: String = panel_spans(&panel, white)
        .iter()
        .map(|span| span.text.as_str())
        .collect();
    assert_eq!(text, "héll");
}

#[test]
fn the_panel_background_is_opaque_in_both_presets() {
    for theme in [Theme::dark(), Theme::light()] {
        let background = panel_background(&theme);
        assert!(
            (background.a - 1.0).abs() < f32::EPSILON,
            "a translucent panel would show the document through itself"
        );
    }
}

/// The addendum finding, pinned: the strip stands on an opaque
/// background in both presets, transparent gutter or not.
#[test]
fn the_strip_background_is_opaque_in_both_presets() {
    for theme in [Theme::dark(), Theme::light()] {
        let background = strip_background(&theme);
        assert!(
            (background.a - 1.0).abs() < f32::EPSILON,
            "a translucent strip would sit its text on document text"
        );
    }
}

/// The active tab has to be findable, and it is findable only because its
/// card is a different colour from the band it sits on.
///
/// Both are derived — the band from the theme's current-line band over the
/// background, the card from the background itself — so a theme whose
/// current-line colour is transparent, or a future edit that pointed both
/// derivations at the same field, would leave a strip on which the tab in
/// front is indistinguishable from every other. Nothing else in the face
/// would fail; the strip would simply stop saying which file you are in.
#[test]
fn the_active_tab_is_distinguishable_from_the_band_in_every_preset() {
    let lights = classic_light_faces().expect("the shipped light themes load");
    let themes = std::iter::once(Theme::dark()).chain(lights.into_iter().map(|(_, it)| it));
    for theme in themes {
        let band = strip_background(&theme);
        let card = super::paint::tab_card_color(&theme);
        let distance = (band.r - card.r).abs() + (band.g - card.g).abs() + (band.b - card.b).abs();
        assert!(
            distance > 0.01,
            "{}: the active tab's card is the same colour as the band under it",
            theme.name
        );
        assert!(
            (card.a - 1.0).abs() < f32::EPSILON,
            "{}: a translucent card would show document text through the tab",
            theme.name
        );
    }
}

/// Every colour the strip's text is drawn in stands on the band opaquely,
/// so no label can be washed out by a theme carrying alpha on its
/// foreground.
#[test]
fn every_tab_text_colour_is_opaque_and_distinct_from_the_band() {
    for theme in [Theme::dark(), Theme::light()] {
        let band = strip_background(&theme);
        let colors = super::paint::tab_colors(&theme);
        for (name, color) in [
            ("active", colors.active),
            ("inactive", colors.inactive),
            ("close", colors.close),
            ("dirty", colors.dirty),
        ] {
            assert!(
                (color.a - 1.0).abs() < f32::EPSILON,
                "{}: the {name} colour is translucent",
                theme.name
            );
            let distance =
                (band.r - color.r).abs() + (band.g - color.g).abs() + (band.b - color.b).abs();
            assert!(
                distance > 0.01,
                "{}: the {name} colour is invisible against the band",
                theme.name
            );
        }
    }
}

#[test]
fn the_hairline_is_opaque_in_both_presets() {
    for theme in [Theme::dark(), Theme::light()] {
        let hairline = hairline_color(&theme);
        assert!((hairline.a - 1.0).abs() < f32::EPSILON);
    }
}

/// D-4's three black-ink constants, and the dark chrome they must not
/// disturb.
///
/// ⭐ The point of each is stated as a *relation*, not as a number. A test
/// that only asserted `0.16` would still pass if the dark side moved to
/// `0.16` too, at which point the light chrome has stopped being a
/// separate decision and is just the dark chrome with the same values.
#[test]
fn the_light_chrome_takes_its_own_ink_constants() {
    let dark = Theme::dark();
    let light = Theme::light();

    // The dark chrome is exactly what the web demo specifies, unmoved.
    let dark_shadow = super::metrics::shadow_for(&dark);
    assert!((dark_shadow.offset_y - 16.0).abs() < f32::EPSILON);
    assert!((dark_shadow.blur - 48.0).abs() < f32::EPSILON);
    assert!((dark_shadow.alpha - 0.55).abs() < f32::EPSILON);
    assert!((super::metrics::backdrop_alpha(&dark) - 0.45).abs() < f32::EPSILON);

    // The light shadow is hard and close where the dark one is a smear:
    // every one of the three moves, and all three move *down*.
    let light_shadow = super::metrics::shadow_for(&light);
    assert!(light_shadow.offset_y < dark_shadow.offset_y);
    assert!(light_shadow.blur < dark_shadow.blur);
    assert!(light_shadow.alpha < dark_shadow.alpha);
    assert!(
        light_shadow.blur < 4.0,
        "the era's shadow is a solid offset rectangle, not a blur"
    );

    // ⚠️ The dim must leave the page recognisably the page. 0.45 of black
    // over `#EFEFEF` composites darker than the *dark* theme's own
    // document background, which would make the modal the one thing on
    // screen that inverts when you switch to light.
    let dim = super::metrics::backdrop_alpha(&light);
    assert!(dim < super::metrics::backdrop_alpha(&dark));
    let dimmed = 1.0 - dim;
    assert!(
        light.editor.background.r * dimmed > dark.editor.background.r,
        "the dimmed light page is darker than the dark theme's page"
    );
}

/// The frame is one *logical* pixel in light and one *physical* pixel in
/// dark, which are the same thing only at 1×.
///
/// ⚠️ Asserted at a scale factor above 1, because at 1× the ruling is
/// invisible: both spellings give 1.0, and a frame that ignored the theme
/// entirely would pass. The Retina case is the whole point of D-4's third
/// item.
#[test]
fn the_light_frame_is_a_logical_pixel_and_the_dark_one_is_not() {
    let dark = Theme::dark();
    let light = Theme::light();

    assert!((super::metrics::frame_width(&dark, 1.0) - 1.0).abs() < f32::EPSILON);
    assert!((super::metrics::frame_width(&light, 1.0) - 1.0).abs() < f32::EPSILON);

    assert!(
        (super::metrics::frame_width(&dark, 2.0) - 1.0).abs() < f32::EPSILON,
        "the dark frame stays one device pixel however dense the display"
    );
    assert!(
        (super::metrics::frame_width(&light, 2.0) - 2.0).abs() < f32::EPSILON,
        "a Platinum frame is 1 px at 1×, so it is 2 physical on Retina"
    );

    // A frame can never vanish, whatever the scale.
    assert!(super::metrics::frame_width(&light, 0.25) >= 1.0);
}

/// D-5 moved the border's alpha out of this file and into the theme. The
/// dark chrome must not have moved with it.
///
/// ⭐ This is the discrimination proof for that refactor, and it is worth
/// stating because the change is invisible from the outside: the field
/// could have been added with any plausible value, every test above would
/// still pass, and the only symptom would be a frame that got heavier or
/// lighter in a face nobody re-screenshotted. So the old derivation is
/// recomputed here, by hand, from the constant this file used to hold —
/// `foreground` at 0.18 — and the shipped result must equal it.
#[test]
fn the_dark_panel_border_is_the_derivation_it_replaced() {
    let theme = Theme::dark();
    let foreground = theme.editor.foreground;
    let derived = super::paint::composite(
        Color::new(foreground.r, foreground.g, foreground.b, 0.18),
        super::paint::panel_background(&theme),
    );
    assert_eq!(
        hairline_color(&theme),
        derived,
        "the dark hairline moved when D-5 made the border a theme field"
    );
}

/// And the light face must genuinely have taken the crisper frame the
/// ruling asked for, rather than inheriting the dark alpha through a
/// missed field.
///
/// Stated as "further from its panel than the dark one is from its own",
/// which is the thing the ruling is actually about — a Platinum frame is
/// a rule you can see, where 0.18 is a modern half-tone. A bare `assert_ne`
/// against the dark value would pass on any difference at all, including
/// one in the wrong direction.
#[test]
fn every_light_preset_frames_its_panels_harder_than_the_dark_one_does() {
    let dark = Theme::dark();
    let separation = |theme: &Theme| {
        let panel = super::paint::panel_background(theme);
        let hairline = hairline_color(theme);
        (panel.r - hairline.r).abs() + (panel.g - hairline.g).abs() + (panel.b - hairline.b).abs()
    };
    let dark_separation = separation(&dark);

    // `classic_light_faces` opens with `Theme::light()` under the slug
    // `platinum`, which is the one a user actually gets, and follows it
    // with the two that ship as files under `themes/`. A future ruling
    // that pointed `light()` at a different variant would change what the
    // first entry is without dropping anything out of this sweep.
    for (_, theme) in classic_light_faces().expect("the shipped light themes load") {
        assert!(
            separation(&theme) > dark_separation,
            "{}: its frame is no crisper than the dark preset's, so the \
             0.55/0.85 the ruling asked for did not reach the screen",
            theme.name
        );
    }
}
