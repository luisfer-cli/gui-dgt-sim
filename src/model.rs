//! The chess model: squares, pieces and FEN parsing.
//!
//! Field numbering follows the DGT convention used by `dgt-sim`: field 0 is
//! `a8` and field 63 is `h1`, which is also the order of a FEN placement, so no
//! rows ever need reversing.

/// A board square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Square {
    /// 0 = file `a`, 7 = file `h`.
    pub file: u8,
    /// 0 = rank 1, 7 = rank 8.
    pub rank: u8,
}

impl Square {
    /// Board field index: `a8` = 0 … `h1` = 63.
    pub const fn field(self) -> usize {
        (7 - self.rank as usize) * 8 + self.file as usize
    }

    /// Inverse of [`Square::field`].
    pub const fn from_field(field: usize) -> Self {
        Self {
            file: (field % 8) as u8,
            rank: (7 - field / 8) as u8,
        }
    }

    /// Algebraic name such as `e4`.
    pub fn name(self) -> String {
        format!("{}{}", (b'a' + self.file) as char, self.rank + 1)
    }
}

/// Piece colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

/// Piece role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

/// A piece, colour plus role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: Kind,
}

impl Piece {
    pub const fn new(color: Color, kind: Kind) -> Self {
        Self { color, kind }
    }

    /// FEN letter: uppercase for white, lowercase for black.
    pub fn fen_char(self) -> char {
        let c = match self.kind {
            Kind::Pawn => 'p',
            Kind::Knight => 'n',
            Kind::Bishop => 'b',
            Kind::Rook => 'r',
            Kind::Queen => 'q',
            Kind::King => 'k',
        };
        if self.color == Color::White {
            c.to_ascii_uppercase()
        } else {
            c
        }
    }

    /// Parse a FEN letter (`N` = white knight, `n` = black knight).
    pub fn from_fen_char(c: char) -> Option<Self> {
        let kind = match c.to_ascii_lowercase() {
            'p' => Kind::Pawn,
            'n' => Kind::Knight,
            'b' => Kind::Bishop,
            'r' => Kind::Rook,
            'q' => Kind::Queen,
            'k' => Kind::King,
            _ => return None,
        };
        let color = if c.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        Some(Self { color, kind })
    }

    /// A filled Unicode glyph (U+265A..U+265F). The caller paints the colour,
    /// so the same glyph is used for both sides.
    pub fn glyph(self) -> char {
        match self.kind {
            Kind::King => '\u{265A}',
            Kind::Queen => '\u{265B}',
            Kind::Rook => '\u{265C}',
            Kind::Bishop => '\u{265D}',
            Kind::Knight => '\u{265E}',
            Kind::Pawn => '\u{265F}',
        }
    }

    /// Fallback letter, used when no font with the chess glyphs is available.
    pub fn letter(self) -> char {
        match self.kind {
            Kind::King => 'K',
            Kind::Queen => 'Q',
            Kind::Rook => 'R',
            Kind::Bishop => 'B',
            Kind::Knight => 'N',
            Kind::Pawn => 'P',
        }
    }
}

/// A whole board, indexed by DGT field (`a8` = 0).
pub type Board = [Option<Piece>; 64];

/// The standard starting position.
pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// An empty board FEN placement.
pub const EMPTY_FEN: &str = "8/8/8/8/8/8/8/8";

/// Parse a full or placement-only FEN into a board. Returns `None` when the
/// placement is malformed.
pub fn parse_fen(fen: &str) -> Option<Board> {
    let placement = fen.split_whitespace().next()?;
    let rows: Vec<&str> = placement.split('/').collect();
    if rows.len() != 8 {
        return None;
    }
    let mut board: Board = [None; 64];
    for (row_index, row) in rows.iter().enumerate() {
        let rank = (7 - row_index) as u8;
        let mut file = 0usize;
        for ch in row.chars() {
            if let Some(digit) = ch.to_digit(10) {
                if digit == 0 {
                    return None;
                }
                file += digit as usize;
            } else {
                if file >= 8 {
                    return None;
                }
                board[Square {
                    file: file as u8,
                    rank,
                }
                .field()] = Piece::from_fen_char(ch);
                file += 1;
            }
        }
        if file != 8 {
            return None;
        }
    }
    Some(board)
}

/// The standard starting position as a board.
pub fn start_board() -> Board {
    parse_fen(START_FEN).expect("the start FEN is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_and_square_roundtrip() {
        for field in 0..64 {
            assert_eq!(Square::from_field(field).field(), field);
        }
        assert_eq!(Square { file: 0, rank: 7 }.name(), "a8");
        assert_eq!(Square { file: 4, rank: 3 }.name(), "e4");
    }

    #[test]
    fn start_position_is_parsed_correctly() {
        let board = start_board();
        assert_eq!(board[0], Some(Piece::new(Color::Black, Kind::Rook)));
        assert_eq!(board[63], Some(Piece::new(Color::White, Kind::Rook)));
        assert_eq!(board[Square { file: 4, rank: 3 }.field()], None);
        assert_eq!(
            board[Square { file: 4, rank: 1 }.field()],
            Some(Piece::new(Color::White, Kind::Pawn))
        );
    }

    #[test]
    fn malformed_fens_are_rejected() {
        assert!(parse_fen("").is_none());
        assert!(parse_fen("8/8/8/8/8/8/8").is_none());
        assert!(parse_fen("9/8/8/8/8/8/8/8").is_none());
        assert!(parse_fen("ppppppppp/8/8/8/8/8/8/8").is_none());
    }
}
