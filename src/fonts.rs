//! Loads a system font that contains the Unicode chess glyphs (U+2654..U+265F).
//!
//! egui's bundled fonts do not include the chess symbols, so the board would
//! show blank squares without this. When no suitable font is found the board
//! falls back to drawing letters.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Candidate fonts, in order of preference. All of them contain the chess
/// symbols; the first that exists is used.
const CANDIDATES: &[&str] = &[
    "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/gnu-free/FreeSerif.otf",
    "/usr/share/fonts/truetype/freefont/FreeSerif.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/Library/Fonts/Arial Unicode.ttf",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
];

/// How pieces should be drawn.
#[derive(Clone)]
pub struct PieceFont {
    /// The font family to use for piece text.
    pub family: FontFamily,
    /// Whether that family really contains the chess glyphs.
    pub glyphs: bool,
}

/// Register the chess font and return how to draw pieces.
pub fn install(ctx: &egui::Context) -> PieceFont {
    let mut fonts = FontDefinitions::default();
    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let name = "dgt_chess".to_owned();
        fonts
            .font_data
            .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
        fonts
            .families
            .insert(FontFamily::Name("dgt_chess".into()), vec![name.clone()]);
        // Also a last-resort fallback, so stray chess glyphs elsewhere render.
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(name);
        ctx.set_fonts(fonts);
        return PieceFont {
            family: FontFamily::Name("dgt_chess".into()),
            glyphs: true,
        };
    }
    ctx.set_fonts(fonts);
    PieceFont {
        family: FontFamily::Proportional,
        glyphs: false,
    }
}
