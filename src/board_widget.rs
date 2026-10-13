//! The chessboard widget: painting plus drag-and-drop interaction.
//!
//! The widget is a pure function of the board state; the caller owns the drag
//! and selection state so it survives across frames.

use egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::fonts::PieceFont;
use crate::model::{Board, Color, Piece, Square};

/// What the user did on the board.
#[derive(Debug, Clone, Copy)]
pub enum BoardAction {
    /// Move a piece from one square to another (played as a DGT move).
    Move { from: Square, to: Square },
    /// Paint a piece onto a square.
    Place { square: Square, piece: Piece },
    /// Empty a square.
    Clear { square: Square },
}

/// The active palette choice: paint a piece, or erase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Palette {
    Piece(Piece),
    Eraser,
}

/// A drag in progress, kept by the caller between frames.
pub struct DragState {
    pub from: Square,
    pub piece: Piece,
    /// Latest pointer position, in screen coordinates.
    pub pointer: Pos2,
}

const LIGHT: Color32 = Color32::from_rgb(0xE9, 0xEC, 0xD3);
const DARK: Color32 = Color32::from_rgb(0x6F, 0x8B, 0x4E);
const LAST_MOVE: Color32 = Color32::from_rgba_premultiplied(0xF6, 0xE0, 0x6A, 0x88);
const SELECTED: Color32 = Color32::from_rgba_premultiplied(0xF6, 0xE0, 0x6A, 0xB8);
const HOVER: Color32 = Color32::from_rgb(0x2B, 0x2B, 0x2B);
const COORD: Color32 = Color32::from_rgb(0x4A, 0x4A, 0x4A);

/// Draw the board and handle interaction. Returns the action the user produced,
/// if any.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    board: &Board,
    flipped: bool,
    palette: Option<Palette>,
    drag: &mut Option<DragState>,
    selected: &mut Option<Square>,
    last_move: Option<(Square, Square)>,
    piece_font: &PieceFont,
) -> Option<BoardAction> {
    let available = ui.available_size();
    let size = available.x.min(available.y).clamp(180.0, 780.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let cell = size / 8.0;

    let cell_rect = |square: Square| -> Rect {
        let col = if flipped {
            7 - square.file
        } else {
            square.file
        };
        let row = if flipped {
            square.rank
        } else {
            7 - square.rank
        };
        Rect::from_min_size(
            rect.min + Vec2::new(col as f32 * cell, row as f32 * cell),
            Vec2::splat(cell),
        )
    };

    let square_at = |pos: Pos2| -> Option<Square> {
        if !rect.contains(pos) {
            return None;
        }
        let local = pos - rect.min;
        let col = (local.x / cell).floor();
        let row = (local.y / cell).floor();
        if !(0.0..8.0).contains(&col) || !(0.0..8.0).contains(&row) {
            return None;
        }
        let (col, row) = (col as u8, row as u8);
        let (file, rank) = if flipped {
            (7 - col, row)
        } else {
            (col, 7 - row)
        };
        Some(Square { file, rank })
    };

    let pointer_pos = response
        .interact_pointer_pos()
        .or_else(|| ui.ctx().pointer_latest_pos());
    let hovered = response.hover_pos().and_then(square_at);

    // ---- interaction -------------------------------------------------------
    let mut action = None;
    match palette {
        // Palette mode: a click paints or erases.
        Some(palette) => {
            if response.clicked()
                && let Some(square) = pointer_pos.and_then(square_at)
            {
                action = Some(match palette {
                    Palette::Eraser => BoardAction::Clear { square },
                    Palette::Piece(piece) => BoardAction::Place { square, piece },
                });
            }
        }
        // Move mode: drag a piece, or click-click to move it.
        None => {
            if response.drag_started()
                && let Some(origin) = ui
                    .input(|input| input.pointer.press_origin())
                    .and_then(square_at)
                && let Some(piece) = board[origin.field()]
            {
                *drag = Some(DragState {
                    from: origin,
                    piece,
                    pointer: pointer_pos.unwrap_or(rect.center()),
                });
                *selected = Some(origin);
            }
            if response.dragged()
                && let (Some(state), Some(pos)) = (drag.as_mut(), pointer_pos)
            {
                state.pointer = pos;
            }
            if response.drag_stopped()
                && let Some(state) = drag.take()
            {
                let release = pointer_pos.unwrap_or(state.pointer);
                if let Some(to) = square_at(release)
                    && to != state.from
                {
                    action = Some(BoardAction::Move {
                        from: state.from,
                        to,
                    });
                }
                *selected = None;
            }
            if response.clicked()
                && let Some(square) = pointer_pos.and_then(square_at)
            {
                match *selected {
                    None => *selected = Some(square),
                    Some(sel) if sel == square => *selected = None,
                    Some(sel) => {
                        action = Some(BoardAction::Move {
                            from: sel,
                            to: square,
                        });
                        *selected = None;
                    }
                }
            }
        }
    }

    // ---- painting ----------------------------------------------------------
    let painter = ui.painter_at(rect);
    for field in 0..64 {
        let square = Square::from_field(field);
        let light = (square.file + square.rank) % 2 == 1;
        painter.rect_filled(
            cell_rect(square),
            CornerRadius::ZERO,
            if light { LIGHT } else { DARK },
        );
    }

    if let Some((from, to)) = last_move {
        for square in [from, to] {
            painter.rect_filled(cell_rect(square), CornerRadius::ZERO, LAST_MOVE);
        }
    }
    if let Some(sel) = *selected {
        painter.rect_filled(cell_rect(sel), CornerRadius::ZERO, SELECTED);
    }
    if let Some(hovered) = hovered {
        let stroke = Stroke::new(2.0, HOVER);
        painter.rect_stroke(
            cell_rect(hovered),
            CornerRadius::ZERO,
            stroke,
            StrokeKind::Inside,
        );
    }

    draw_coordinates(&painter, rect, cell, flipped);

    let piece_size = if piece_font.glyphs {
        cell * 0.82
    } else {
        cell * 0.5
    };
    let font = FontId::new(piece_size, piece_font.family.clone());
    let dragged_from = drag.as_ref().map(|state| state.from);
    for (field, piece) in board.iter().enumerate() {
        let square = Square::from_field(field);
        if dragged_from == Some(square) {
            continue;
        }
        if let Some(piece) = *piece {
            draw_piece(
                &painter,
                cell_rect(square).center(),
                piece,
                &font,
                cell,
                piece_font.glyphs,
            );
        }
    }
    // The dragged piece follows the cursor, drawn unclipped so it does not
    // vanish when the pointer leaves the board.
    if let Some(state) = drag.as_ref() {
        draw_piece(
            ui.painter(),
            state.pointer,
            state.piece,
            &font,
            cell,
            piece_font.glyphs,
        );
    }

    action
}

fn draw_coordinates(painter: &Painter, rect: Rect, cell: f32, flipped: bool) {
    let size = (cell * 0.24).clamp(9.0, 16.0);
    let font = FontId::proportional(size);
    for i in 0..8u8 {
        let file = if flipped { 7 - i } else { i };
        let col = if flipped { 7 - file } else { file };
        let x = rect.min.x + col as f32 * cell + 3.0;
        let y = rect.max.y - 2.0;
        painter.text(
            Pos2::new(x, y),
            Align2::LEFT_BOTTOM,
            (b'a' + file) as char,
            font.clone(),
            COORD,
        );

        let rank = if flipped { 7 - i } else { i };
        let row = if flipped { rank } else { 7 - rank };
        let x = rect.min.x + 3.0;
        let y = rect.min.y + row as f32 * cell + 2.0;
        painter.text(
            Pos2::new(x, y),
            Align2::LEFT_TOP,
            (rank + 1).to_string(),
            font.clone(),
            COORD,
        );
    }
}

fn draw_piece(
    painter: &Painter,
    center: Pos2,
    piece: Piece,
    font: &FontId,
    cell: f32,
    glyphs: bool,
) {
    let (fill, shadow) = match piece.color {
        Color::White => (
            Color32::from_rgb(0xF8, 0xF8, 0xF8),
            Color32::from_rgba_premultiplied(0, 0, 0, 120),
        ),
        Color::Black => (
            Color32::from_rgb(0x1E, 0x1E, 0x1E),
            Color32::from_rgba_premultiplied(255, 255, 255, 80),
        ),
    };
    let text = if glyphs {
        piece.glyph().to_string()
    } else {
        piece.letter().to_string()
    };
    let offset = (cell * 0.035).max(1.0);
    painter.text(
        center + Vec2::new(offset, offset),
        Align2::CENTER_CENTER,
        &text,
        font.clone(),
        shadow,
    );
    painter.text(center, Align2::CENTER_CENTER, &text, font.clone(), fill);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::PieceFont;
    use crate::model::{self, Square};
    use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

    /// A 400 px board makes every cell exactly 50 px.
    const BOARD: f32 = 400.0;
    const CELL: f32 = BOARD / 8.0;

    fn piece_font() -> PieceFont {
        PieceFont {
            family: egui::FontFamily::Proportional,
            glyphs: false,
        }
    }

    /// Screen position of the centre of a square on an unflipped 400 px board.
    fn center(square: Square) -> Pos2 {
        let col = square.file as f32;
        let row = (7 - square.rank) as f32;
        Pos2::new((col + 0.5) * CELL, (row + 0.5) * CELL)
    }

    /// Replay a sequence of pointer events against the widget and return the
    /// last action it produced.
    fn play(
        board: &Board,
        palette: Option<Palette>,
        frames: Vec<Vec<Event>>,
    ) -> Option<BoardAction> {
        let ctx = Context::default();
        ctx.set_fonts(egui::FontDefinitions::empty());
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::splat(BOARD));
        let font = piece_font();
        let mut drag = None;
        let mut selected = None;
        let mut action = None;

        for events in frames {
            let input = RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                if let Some(next) =
                    show(ui, board, false, palette, &mut drag, &mut selected, None, &font)
                {
                    action = Some(next);
                }
            });
            output.drop_without_applying_deltas();
        }
        action
    }

    fn press(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn dragging_a_pawn_emits_a_move() {
        let board = model::start_board();
        let e2 = Square { file: 4, rank: 1 };
        let e4 = Square { file: 4, rank: 3 };
        let action = play(
            &board,
            None,
            vec![
                vec![],
                vec![Event::PointerMoved(center(e2)), press(center(e2), true)],
                vec![Event::PointerMoved(center(e4))],
                vec![Event::PointerMoved(center(e4)), press(center(e4), false)],
            ],
        );
        match action {
            Some(BoardAction::Move { from, to }) => {
                assert_eq!(from, e2);
                assert_eq!(to, e4);
            }
            other => panic!("expected a move, got {other:?}"),
        }
    }

    #[test]
    fn dragging_from_an_empty_square_does_nothing() {
        let board = model::start_board();
        let e4 = Square { file: 4, rank: 3 };
        let e5 = Square { file: 4, rank: 4 };
        let action = play(
            &board,
            None,
            vec![
                vec![],
                vec![Event::PointerMoved(center(e4)), press(center(e4), true)],
                vec![Event::PointerMoved(center(e5))],
                vec![Event::PointerMoved(center(e5)), press(center(e5), false)],
            ],
        );
        assert!(action.is_none());
    }

    #[test]
    fn click_click_also_moves_a_piece() {
        let board = model::start_board();
        let g1 = Square { file: 6, rank: 0 };
        let f3 = Square { file: 5, rank: 2 };
        let action = play(
            &board,
            None,
            vec![
                vec![],
                vec![Event::PointerMoved(center(g1)), press(center(g1), true)],
                vec![Event::PointerMoved(center(g1)), press(center(g1), false)],
                vec![Event::PointerMoved(center(f3)), press(center(f3), true)],
                vec![Event::PointerMoved(center(f3)), press(center(f3), false)],
            ],
        );
        match action {
            Some(BoardAction::Move { from, to }) => {
                assert_eq!(from, g1);
                assert_eq!(to, f3);
            }
            other => panic!("expected a move, got {other:?}"),
        }
    }

    #[test]
    fn palette_click_places_and_erases() {
        let board = model::start_board();
        let d5 = Square { file: 3, rank: 4 };
        let piece = Piece::new(Color::Black, crate::model::Kind::Knight);
        let action = play(
            &board,
            Some(Palette::Piece(piece)),
            vec![
                vec![],
                vec![Event::PointerMoved(center(d5)), press(center(d5), true)],
                vec![Event::PointerMoved(center(d5)), press(center(d5), false)],
            ],
        );
        match action {
            Some(BoardAction::Place { square, piece: placed }) => {
                assert_eq!(square, d5);
                assert_eq!(placed, piece);
            }
            other => panic!("expected a place, got {other:?}"),
        }

        let e2 = Square { file: 4, rank: 1 };
        let action = play(
            &board,
            Some(Palette::Eraser),
            vec![
                vec![],
                vec![Event::PointerMoved(center(e2)), press(center(e2), true)],
                vec![Event::PointerMoved(center(e2)), press(center(e2), false)],
            ],
        );
        match action {
            Some(BoardAction::Clear { square }) => assert_eq!(square, e2),
            other => panic!("expected a clear, got {other:?}"),
        }
    }
}

