//! The shot recipes: one function per frame, plus the run that writes the
//! whole set.
//!
//! Split from the harness root so that "what a frame *is*" and "how a frame is
//! made and encoded" are two files. Everything about the GPU, the readback and
//! the PNG lives in the parent; a child module can see its parent's private
//! items, so nothing had to be made public to draw the line.
//!
//! Which palette each frame wears, and why the `bridge` frame wears nobody's,
//! is in the parent's module documentation.

use std::path::{Path, PathBuf};

use iridium_config::test_support::classic_light_faces;
use iridium_desktop::command_palette::CommandPalette;
use iridium_desktop::context_menu::ContextMenu;
use iridium_desktop::history_overlay::HistoryPanel;
use iridium_desktop::overlay::{OverlayPainter, PanelFit, PanelKind, StripContent};
use iridium_desktop::search::SearchOverlay;
use iridium_desktop::tab_strip::{TabItem, TabStripContent};
use iridium_desktop::units::u32_to_f32;
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, Modifiers};

use super::{
    Chrome, FONT, FONT_SIZE, Gpu, HEIGHT, Palette, SCALE, WIDTH, check_clear_colour, chord,
    editor_with_document, headless_gpu, press, shoot, shot_dir,
};

/// The bare document: the surface, the gutter, the current-line band under
/// the caret, and the code in the theme's own syntax colours.
///
/// The only shot that asserts a pixel, because it is the only one with an
/// uncovered page: [`check_clear_colour`] reads its corner.
fn document_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let editor = editor_with_document(theme, palette)?;
    let pixels = shoot(gpu, overlay, theme, &editor, palette, Chrome::NONE, path)?;
    check_clear_colour(
        &pixels,
        theme.editor.background,
        &format!("{} ({})", path.display(), theme.name),
    )
}

/// A live multi-line selection over the page — `selection` and
/// `selection_inactive`'s hue judged against the code it covers.
///
/// Not a current-line shot, and deliberately not named one: the compositor's
/// only line-background pass is fed by `line_backgrounds_mut`, which nothing
/// in this face populates, so `theme.editor.current_line` never reaches the
/// document surface. It reaches pixels through the overlay's panel and strip
/// derivation instead, which is what the `palette`, `search` and `history`
/// frames show.
fn selection_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    for _ in 0..3 {
        let _ = editor.handle_key(&chord(KeyCode::Down, Modifiers::shift()));
    }
    let _ = editor.handle_key(&chord(KeyCode::Right, Modifiers::shift()));
    shoot(gpu, overlay, theme, &editor, palette, Chrome::NONE, path)?;
    Ok(())
}

/// The command palette: a query typed, the selection moved off the first row,
/// over a document with a visible text selection.
fn palette_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    for _ in 0..2 {
        let _ = editor.handle_key(&chord(KeyCode::Down, Modifiers::shift()));
    }
    let mut command_palette = CommandPalette::new();
    command_palette.open();
    let mru = CommandMru::default();
    for character in "se".chars() {
        let _ = command_palette.handle_key(&press(KeyCode::Char(character)), &editor, &mru);
    }
    let _ = command_palette.handle_key(&press(KeyCode::Down), &editor, &mru);
    let content = command_palette.content(&editor, &mru, &editor.state().theme, fit);
    let chrome = Chrome {
        tabs: None,
        strip: None,
        panels: &[(PanelKind::Palette, &content)],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The same palette with a row under the pointer as well as a row selected —
/// the frame that answers whether the hover band can be *seen*, and whether it
/// can be told apart from the selection.
///
/// Everything else about the hover is arithmetic and is already tested on the
/// CPU: that it composites at the overlay's hover strength, that it lands on
/// the row the pointer is over, that it yields to the selection when the two
/// coincide. None of that is the question. The question is whether a band
/// drawn at four
/// tenths of the selection's alpha is visible at all against a panel
/// background, and whether at that strength it reads as *hover* rather than as
/// a second, weaker selection. Only an eye answers that, which is why this
/// frame exists.
///
/// ⚠️ **The hovered row is found in the composed content, not written in as a
/// literal.** An index past the end, or on a separator, is silently skipped by
/// the painter — so a hard-coded row would produce a frame with no band on it
/// and nothing anywhere would say so. This picks the first drawable row that
/// is neither the selection nor a separator, and fails the run outright if
/// there is none.
///
/// Row 0 is skipped along with them: it is the query, which a press treats as
/// a no-op, and a band under the text being typed would be a frame of a state
/// the pointer cannot put the panel in.
fn hover_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let editor = editor_with_document(theme, palette)?;
    let mut command_palette = CommandPalette::new();
    command_palette.open();
    let mru = CommandMru::default();
    for character in "se".chars() {
        let _ = command_palette.handle_key(&press(KeyCode::Char(character)), &editor, &mru);
    }
    let _ = command_palette.handle_key(&press(KeyCode::Down), &editor, &mru);
    let mut content = command_palette.content(&editor, &mru, &editor.state().theme, fit);
    let hovered = content
        .rows
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, row)| !row.selected && !row.separator)
        .map(|(index, _)| index)
        .ok_or_else(|| {
            "the palette composed no row that could carry a hover band, so the frame \
             would have shown nothing"
                .to_string()
        })?;
    content.hovered = Some(hovered);
    let chrome = Chrome {
        tabs: None,
        strip: None,
        panels: &[(PanelKind::Palette, &content)],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The context menu hung from a click in the document — the frame that
/// answers what no CPU test can: row height and padding against the
/// palette's, the separator rules, and (in the read-only shot) the exact
/// alpha a greyed verb is drawn at.
fn menu_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    read_only: bool,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    // A selection under the click, which is what a real right-press finds.
    for _ in 0..2 {
        let _ = editor.handle_key(&chord(KeyCode::Right, Modifiers::shift()));
    }
    editor.state_mut().read_only = read_only;
    let menu = ContextMenu::open(&editor, 0.34 * u32_to_f32(WIDTH), 0.30 * u32_to_f32(HEIGHT));
    let content = menu.content(&editor.state().theme, fit);
    let chrome = Chrome {
        tabs: None,
        strip: None,
        panels: &[(PanelKind::Menu, &content)],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The search panel above the strip, a query with live matches — where
/// `search_match` and `search_match_current` are judged against real text.
fn search_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    let mut search = SearchOverlay::new();
    search.open(&mut editor);
    for character in "entry".chars() {
        let _ = search.handle_key(&press(KeyCode::Char(character)), &mut editor);
    }
    let strip = StripContent {
        text: "chrome preview — the strip stands on an honest background".to_string(),
        caret_column: None,
        is_error: false,
    };
    let content = search.content(&editor, &editor.state().theme, fit);
    let chrome = Chrome {
        tabs: None,
        strip: Some(&strip),
        panels: &[(PanelKind::Search, &content)],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The undo tree, with a real branch to badge — the panel's dim rows are the
/// `line_number`-class grey.
fn history_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    for character in "abc".chars() {
        let _ = editor.handle_key(&press(KeyCode::Char(character)));
    }
    let _ = editor.handle_key(&chord(KeyCode::Char('z'), Modifiers::ctrl()));
    for character in "xy".chars() {
        let _ = editor.handle_key(&press(KeyCode::Char(character)));
    }
    let mut history = HistoryPanel::new();
    history.open();
    let content = history.content(&editor, &editor.state().theme, fit);
    let chrome = Chrome {
        tabs: None,
        strip: None,
        panels: &[(PanelKind::History, &content)],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The tab strip along the top edge, with the document pushed down under it.
///
/// The frame that answers what no CPU test can about the strip: whether the
/// active tab's card reads against the band, whether an inactive label is
/// legible at its alpha, whether the dirty dot is findable, and — the thing
/// the round-trip test proves arithmetically and nobody has yet *seen* —
/// whether the document really does start below the band rather than under
/// it.
fn tab_strip_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let editor = editor_with_document(theme, palette)?;
    let tab = |label: &str, is_active: bool, is_dirty: bool| TabItem {
        label: label.to_owned(),
        is_active,
        is_dirty,
    };
    let tabs = TabStripContent {
        tabs: vec![
            tab("main.rs", false, false),
            tab("compositor.rs", true, true),
            tab("Cargo.toml", false, false),
            // Longer than the budget: the frame that shows where the cut
            // falls and what the ellipsis looks like beside a real name.
            tab("a-rather-long-file-name.json", false, true),
            tab("untitled", false, false),
        ],
    };
    let chrome = Chrome {
        tabs: Some(&tabs),
        strip: None,
        panels: &[],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// One candidate light variant's six frames, per the light-theme map §4.1.
///
/// Five of them wear the variant's own syntax palette; the sixth is the
/// bridge, which wears nobody's — see the module doc.
fn variant_shots(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    slug: &str,
    theme: &Theme,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let named = |state: &str| out_dir.join(format!("light-{slug}-{state}.png"));

    let editor_path = named("editor");
    document_shot(gpu, overlay, theme, Palette::ThemeSyntax, &editor_path)?;

    let selection_path = named("selection");
    selection_shot(gpu, overlay, theme, Palette::ThemeSyntax, &selection_path)?;

    let palette_path = named("palette");
    palette_shot(
        gpu,
        overlay,
        fit,
        theme,
        Palette::ThemeSyntax,
        &palette_path,
    )?;

    let search_path = named("search");
    search_shot(gpu, overlay, fit, theme, Palette::ThemeSyntax, &search_path)?;

    let history_path = named("history");
    history_shot(
        gpu,
        overlay,
        fit,
        theme,
        Palette::ThemeSyntax,
        &history_path,
    )?;

    let bridge_path = named("bridge");
    document_shot(gpu, overlay, theme, Palette::KeywordBridge, &bridge_path)?;

    Ok(vec![
        editor_path,
        selection_path,
        palette_path,
        search_path,
        history_path,
        bridge_path,
    ])
}

/// The whole harness: the dark control's three frames, then six per candidate
/// light variant.
pub fn run() -> Result<Vec<PathBuf>, String> {
    let out_dir = shot_dir();
    std::fs::create_dir_all(&out_dir)
        .map_err(|error| format!("cannot create {}: {error}", out_dir.display()))?;
    let gpu = headless_gpu()?;

    let mut overlay = OverlayPainter::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Bgra8Unorm,
        WIDTH,
        HEIGHT,
    )
    .map_err(|error| format!("the overlay painter could not be created: {error}"))?;
    assert!(
        overlay.set_font(FONT_SIZE, FONT.to_vec()),
        "the vendored test font holds no readable face"
    );
    overlay.set_scale(SCALE);

    let fit = overlay
        .panel_fit(WIDTH, HEIGHT)
        .ok_or_else(|| "the shot window cannot fit a panel".to_string())?;
    let mut written = Vec::new();

    // The dark control: the three frames this harness has always produced,
    // under the same theme and the same keyword-bridge source, under the same
    // names — so the light set is judged against an unchanged reference.
    let dark = Theme::dark();
    let palette_path = out_dir.join("chrome-palette.png");
    palette_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        &palette_path,
    )?;
    written.push(palette_path);

    // The same palette with a row under the pointer as well as one selected —
    // the one frame in the set that shows the hover band at all.
    let hover_path = out_dir.join("chrome-hover.png");
    hover_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        &hover_path,
    )?;
    written.push(hover_path);

    let search_path = out_dir.join("chrome-search.png");
    search_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        &search_path,
    )?;
    written.push(search_path);

    let menu_path = out_dir.join("chrome-menu.png");
    menu_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        false,
        &menu_path,
    )?;
    written.push(menu_path);

    // The same menu over a read-only document, where the two mutating verbs
    // are greyed — the only frame that shows the disabled alpha.
    let menu_read_only_path = out_dir.join("chrome-menu-read-only.png");
    menu_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        true,
        &menu_read_only_path,
    )?;
    written.push(menu_read_only_path);

    let history_path = out_dir.join("chrome-history.png");
    history_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        &history_path,
    )?;
    written.push(history_path);

    let tabs_path = out_dir.join("chrome-tabs.png");
    tab_strip_shot(
        &gpu,
        &mut overlay,
        &dark,
        Palette::KeywordBridge,
        &tabs_path,
    )?;
    written.push(tabs_path);

    // The map's three light faces: the ruled preset, plus the two that ship
    // as files under `themes/` and are read from disk here exactly as a user's
    // `--theme` reads them.
    let faces = classic_light_faces()
        .map_err(|error| format!("the shipped light themes under `themes/` must load: {error}"))?;
    for (slug, theme) in faces {
        written.extend(variant_shots(
            &gpu,
            &mut overlay,
            fit,
            slug,
            &theme,
            &out_dir,
        )?);
    }

    Ok(written)
}
