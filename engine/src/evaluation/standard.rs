use serde::{Serialize, Deserialize};
use chess::*;

use crate::evaluation::{Evaluation, Evaluator};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceSquareTable(pub [[i32; 8]; 8]);

impl PieceSquareTable {
    fn key(side: chess::Color, square: chess::Square) -> (usize, usize) {
        let mut rank = square.get_rank().to_index();
        if side == chess::Color::White {
            rank = 7 - rank;
        }
        (rank, square.get_file().to_index())
    }
    
    pub fn get(&self, side: chess::Color, square: chess::Square) -> i32 {
        let (rank, file) = Self::key(side, square);
        self.0[rank][file]
    }

    pub fn set(&mut self, side: chess::Color, square: chess::Square, value: i32) {
        let (rank, file) = Self::key(side, square);
        self.0[rank][file] = value;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceEvalSet<T> {
    pub pawn: T,
    pub knight: T,
    pub bishop: T,
    pub rook: T,
    pub queen: T,
    pub king: T
}

impl<T> PieceEvalSet<T> {
    pub fn get(&self, piece: Piece) -> &T {
        match piece {
            Piece::Pawn => &self.pawn,
            Piece::Knight => &self.knight,
            Piece::Bishop => &self.bishop,
            Piece::Rook => &self.rook,
            Piece::Queen => &self.queen,
            Piece::King => &self.king
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardEvaluator {
    pub piece_values: PieceEvalSet<i32>,
    pub midgame_piece_tables: PieceEvalSet<PieceSquareTable>,
    pub endgame_piece_tables: PieceEvalSet<PieceSquareTable>
}

impl Default for StandardEvaluator {
    fn default() -> Self {
        Self {
            piece_values: PieceEvalSet {
                pawn: 100,
                knight: 320,
                bishop: 330,
                rook: 500,
                queen: 900,
                king: 0
            },
            midgame_piece_tables: PieceEvalSet {
                pawn: PieceSquareTable([
                    [  0,   0,   0,   0,   0,   0,   0,   0],
                    [-23, -11,  -5,  18,  23,   5,  -1, -11],
                    [-61, -49, -14,  -3,  16,  16, -33, -41],
                    [-25, -17, -19,  11,  28,   1, -19, -24],
                    [-52, -17, -17,  18,  11, -18, -28, -70],
                    [-30, -25,  -5, -22,   3, -42,  29,   0],
                    [-37,  -8, -35, -31, -14,  25,  47,   6],
                    [  0,   0,   0,   0,   0,   0,   0,   0]
                ]),
                knight: PieceSquareTable([
                    [-89, -29, -27, -34, -20, -40, -36, -44],
                    [-68, -27,  42,  16,  39,  37,  -4, -27],
                    [-28,   8,  22,  56,  63,  53,  34, -20],
                    [-23,   8,  12,  58,  28,  45,  -3, -10],
                    [-26,   3,  16,  17,  31,  24,  12, -35],
                    [-27, -11,  25,  14,  20,  21,  25, -30],
                    [-29, -23,   1,  -1,   1,  15, -12,   6],
                    [-49, -25, -47, -38, -41, -20, -32, -58]
                ]),
                bishop: PieceSquareTable([
                    [-18,  -9,  -4, -17, -10, -12,  -1, -25],
                    [-28,  18,  -6,  10,  13,  31,  22,   9],
                    [-13,   4,  50,  24,  38,  28,  18,  37],
                    [-29,  -3,  16,  31,  36,  21,  14,   2],
                    [ -8,  17,  20,  44,  53,  25,  19, -18],
                    [ 26,  35,  28,  31,  28,  36,  31,  32],
                    [ -9,  50,  25,  17,  22,  32,  64,  17],
                    [ -6,  -5,  19,  -9, -13, -14,  -1, -21]
                ]),
                rook: PieceSquareTable([
                    [-22, -16, -18, -14, -15,  -9, -11, -10],
                    [-12,  -7,   4,   6,  10,   9,  12,   3],
                    [-16, -18,  -9,  -1,   7,   2,  -7,  -8],
                    [-26, -17, -10,  -2,  -4,  -1,  -6,   4],
                    [-13, -18, -25, -14, -12,  -5,  -5,  -4],
                    [-25, -15, -11,  -4, -10,  -4,  -2,  -7],
                    [-35, -10, -19, -15, -16,   7,   1, -30],
                    [-20, -13,   7,   4,  17,   4, -44, -13]
                ]),
                queen: PieceSquareTable([
                    [-45, -37, -23, -29, -25, -26, -26, -46],
                    [-39, -32, -15, -17, -15,  15,  16,  32],
                    [-25, -13,   6,  17,  14,  31,  33,  33],
                    [-17, -19,  -6, -11,  -1,   2,  -9,  12],
                    [-22,  -5, -13,  -9,   3,  -4,  15,  -8],
                    [-20,   3,  -7,  -7, -12,   8,   7, -10],
                    [-23,   2,   5,  -2,   8,  17,  10,  -3],
                    [ -3, -24, -12,  20, -22, -23, -27, -23]
                ]),
                king: PieceSquareTable([
                    [-33, -46, -43, -46, -51, -52, -53, -38],
                    [-35, -46, -39, -39, -44, -48, -57, -50],
                    [-35, -43, -24, -22, -26, -31, -58, -52],
                    [-33, -37, -14, -18, -17, -23, -48, -48],
                    [-17, -24,  -6, -13, -14, -14, -37, -35],
                    [ -8, -14, -12, -13, -17, -19, -28, -25],
                    [ 20,   0,  -8, -66, -67, -17,   5,  27],
                    [  2,  37,   4, -53,  14, -44,  63,  43]
                ])
            },
            endgame_piece_tables: PieceEvalSet {
                pawn: PieceSquareTable([
                    [  0,   0,   0,   0,   0,   0,   0,   0],
                    [119, 113, 102,  91,  87,  91, 106, 102],
                    [ 58,  61,  60,  46,  47,  50,  52,  50],
                    [ 17,  17,   6,   4,   6,   7,  11,   7],
                    [ -7,   2, -11,  -2,  -3,  -9,  -4, -22],
                    [-10,  -1,  -6,  -7,   1, -11,  14,  -4],
                    [ -5,   7,  -5, -15,  -3,  14,  23,  -3],
                    [  0,   0,   0,   0,   0,   0,   0,   0]
                ]),
                knight: PieceSquareTable([
                    [-84, -54, -38, -29, -33, -50, -50, -79],
                    [-59, -33,   7,  -4,   1,   2, -25, -49],
                    [-32, -10,  11,  23,  16,  18,  -1, -31],
                    [-20,   6,   8,  37,  17,  23,  -1, -21],
                    [-21,  -9,  10,  12,  21,   9,  -8, -28],
                    [-32, -13,   4,   8,  10,   0,  -3, -36],
                    [-38, -29, -18,  -8,  -9,  -8, -25, -17],
                    [-64, -36, -45, -32, -41, -24, -37, -63]
                ]),
                bishop: PieceSquareTable([
                    [-13, -22, -20, -13, -19, -16, -19, -17],
                    [-25,  10,  -2,  -3,   4,   6,  10, -16],
                    [ -6,   6,  23,  18,  18,  20,   7,  12],
                    [ -7,   8,  14,  25,  27,  10,  12,  -6],
                    [ -8,   9,  19,  24,  30,  24,   5, -15],
                    [  8,  13,  20,  20,  28,  20,  10,   3],
                    [-19,  20,   4,  13,  17,  13,  30,  -1],
                    [-19, -18,  -1,  -7, -12,  -9, -20, -19]
                ]),
                rook: PieceSquareTable([
                    [ 24,  19,  21,  20,  21,  11,  11,  11],
                    [ 17,  23,  26,  27,  19,  26,  21,  11],
                    [ 14,  14,  16,  17,  15,  16,  13,   7],
                    [  6,   7,  16,  14,  10,  14,   8,  10],
                    [  3,   4,   7,   9,   5,   3,   0,  -4],
                    [ -9,   0,   2,   4,   1,  -3,  -2,  -6],
                    [-10,   1,   2,   1,   0,   7,   2, -17],
                    [ -7,  -1,   9,  15,   9,   1, -16, -22]
                ]),
                queen: PieceSquareTable([
                    [ -2,   8,  15,  20,  24,  20,   5,  11],
                    [-19, -13,   1,  17,   8,  47,  31,  13],
                    [-23,  -9,  21,  24,  42,  39,  38,  26],
                    [-16,  -1,   6,  16,  27,  31,  11,  19],
                    [-11,   6,   2,  17,  22,  12,  22,   0],
                    [-17,   0,   9,   7,  10,  16,  19,  -4],
                    [-11,   7,   4,  10,  11,   9,   2,  -5],
                    [  1, -17, -11,  -1,  -9, -19, -18, -25]
                ]),
                king: PieceSquareTable([
                    [-43, -28, -21, -19, -15, -11, -22, -38],
                    [-20,  -4,  -4,  -8,  -3,   5,   0,  -9],
                    [-19,   2,   4,   4,   4,  13,  14,  -6],
                    [-25,  -9,   5,   6,   4,  10,   0, -11],
                    [-31, -12,   5,   6,   9,   8,  -4, -19],
                    [-28, -12,   0,   4,   5,   4,  -4, -17],
                    [-22, -11,  -4, -15, -15,   0,   2,  -8],
                    [-43, -12, -12, -41, -16, -31,  -6, -28]
                ])
            }
        }
    }
}

impl Evaluator for StandardEvaluator {
    fn evaluate(&self, board: &Board, ply_index: u8) -> Evaluation {
        match board.status() {
            BoardStatus::Ongoing => {
                let phase = Self::game_phase(&board);
                let white = self.evaluate_for_side(board, chess::Color::White, phase);
                let black = self.evaluate_for_side(board, chess::Color::Black, phase);
                Evaluation::from_centipawns(if board.side_to_move() == Color::White {
                    white - black
                } else {
                    black - white
                })
            },
            BoardStatus::Checkmate => Evaluation::mated_in(ply_index),
            BoardStatus::Stalemate => Evaluation::DRAW
        }
    }

    fn piece_value(&self, piece: Piece) -> Evaluation {
        Evaluation::from_centipawns(*self.piece_values.get(piece))
    }
}

impl StandardEvaluator {
    const MAX_PHASE: u32 = 256;

    fn game_phase(board: &Board) -> u32 {
        macro_rules! game_phase_fn {
            ($($piece:ident=$weight:expr,$count:expr;)*) => {
                const INIT_PHASE: u32 = ($($count * $weight + )* 0) * 2;
                let phase = INIT_PHASE $( - board.pieces(Piece::$piece).popcnt() * $weight)*;
                (phase * Self::MAX_PHASE + (INIT_PHASE / 2)) / INIT_PHASE
            }
        }
        game_phase_fn! {
            Pawn   = 0, 8;
            Knight = 1, 2;
            Bishop = 1, 2;
            Rook   = 2, 2;
            Queen  = 4, 1;
        }
    }

    fn evaluate_for_side(&self, board: &Board, side: Color, phase: u32) -> i32 {
        let mut value = 0;
        let mut midgame_value = 0;
        let mut endgame_value = 0;
        let ally_pieces = *board.color_combined(side);

        for &piece in &ALL_PIECES {
            let pieces = ally_pieces & *board.pieces(piece);
            let piece_value = *self.piece_values.get(piece);
            let midgame_piece_table = self.midgame_piece_tables.get(piece);
            let endgame_piece_table = self.endgame_piece_tables.get(piece);

            value += pieces.popcnt() as i32 * piece_value;
            for square in pieces {
                midgame_value += midgame_piece_table.get(side, square);
                endgame_value += endgame_piece_table.get(side, square);
            }
        }

        midgame_value += value;
        endgame_value += value;
        let phase = phase as i32;
        const MAX_PHASE: i32 = StandardEvaluator::MAX_PHASE as i32;
        (((midgame_value) * (MAX_PHASE - phase)) + ((endgame_value) * phase)) / MAX_PHASE
    }
}