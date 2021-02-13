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
pub struct StandardEvaluator {
    pub pawn: i32,
    pub pawn_table: PieceSquareTable,
    pub knight: i32,
    pub knight_table: PieceSquareTable,
    pub bishop: i32,
    pub bishop_table: PieceSquareTable,
    pub rook: i32,
    pub rook_table: PieceSquareTable,
    pub queen: i32,
    pub queen_table: PieceSquareTable,
    pub king_table: PieceSquareTable,
    pub king_table_endgame: PieceSquareTable
}

impl Default for StandardEvaluator {
    fn default() -> Self {
        Self {
            pawn: 100,
            pawn_table: PieceSquareTable([
                [  0,   0,   0,   0,   0,   0,   0,   0],
                [ 50,  50,  50,  50,  50,  50,  50,  50],
                [ 10,  10,  20,  30,  30,  20,  10,  10],
                [  5,   5,  10,  25,  25,  10,   5,   5],
                [  0,   0,   0,  20,  20,   0,   0,   0],
                [  5,  -5, -10,   0,   0, -10,  -5,   5],
                [  5,  10,  10, -20, -20,  10,  10,   5],
                [  0,   0,   0,   0,   0,   0,   0,   0]
            ]),
            knight: 320,
            knight_table: PieceSquareTable([
                [-50, -40, -30, -30, -30, -30, -40, -50],
                [-40, -20,   0,   0,   0,   0, -20, -40],
                [-30,   0,  10,  15,  15,  10,   0, -30],
                [-30,   5,  15,  20,  20,  15,   5, -30],
                [-30,   0,  15,  20,  20,  15,   0, -30],
                [-30,   5,  10,  15,  15,  10,   5, -30],
                [-40, -20,   0,   5,   5,   0, -20, -40],
                [-50, -40, -30, -30, -30, -30, -40, -50]
            ]),
            bishop: 330,
            bishop_table: PieceSquareTable([
                [-20, -10, -10, -10, -10, -10, -10, -20],
                [-10,   0,   0,   0,   0,   0,   0, -10],
                [-10,   0,   5,  10,  10,   5,   0, -10],
                [-10,   5,   5,  10,  10,   5,   5, -10],
                [-10,   0,  10,  10,  10,  10,   0, -10],
                [-10,  10,  10,  10,  10,  10,  10, -10],
                [-10,   5,   0,   0,   0,   0,   5, -10],
                [-20, -10, -10, -10, -10, -10, -10, -20]
            ]),
            rook: 500,
            rook_table: PieceSquareTable([
                [  0,   0,   0,   0,   0,   0,   0,   0],
                [  5,  10,  10,  10,  10,  10,  10,   5],
                [ -5,   0,   0,   0,   0,   0,   0,  -5],
                [ -5,   0,   0,   0,   0,   0,   0,  -5],
                [ -5,   0,   0,   0,   0,   0,   0,  -5],
                [ -5,   0,   0,   0,   0,   0,   0,  -5],
                [ -5,   0,   0,   0,   0,   0,   0,  -5],
                [  0,   0,   0,   5,   5,   0,   0,   0]
            ]),
            queen: 900,
            queen_table: PieceSquareTable([
                [-20, -10, -10,  -5,  -5, -10, -10, -20],
                [-10,   0,   0,   0,   0,   0,   0, -10],
                [-10,   0,   5,   5,   5,   5,   0, -10],
                [ -5,   0,   5,   5,   5,   5,   0,  -5],
                [  0,   0,   5,   5,   5,   5,   0,  -5],
                [-10,   5,   5,   5,   5,   5,   0, -10],
                [-10,   0,   5,   0,   0,   0,   0, -10],
                [-20, -10, -10,  -5,  -5, -10, -10, -20]
            ]),
            king_table: PieceSquareTable([
                [-30, -40, -40, -50, -50, -40, -40, -30],
                [-30, -40, -40, -50, -50, -40, -40, -30],
                [-30, -40, -40, -50, -50, -40, -40, -30],
                [-30, -40, -40, -50, -50, -40, -40, -30],
                [-20, -30, -30, -40, -40, -30, -30, -20],
                [-10, -20, -20, -20, -20, -20, -20, -10],
                [ 20,  20,   0,   0,   0,   0,  20,  20],
                [ 20,  30,  10,   0,   0,  10,  30,  20]
            ]),
            king_table_endgame: PieceSquareTable([
                [-50, -40, -30, -20, -20, -30, -40, -50],
                [-30, -20, -10,   0,   0, -10, -20, -30],
                [-30, -10,  20,  30,  30,  20, -10, -30],
                [-30, -10,  30,  40,  40,  30, -10, -30],
                [-30, -10,  30,  40,  40,  30, -10, -30],
                [-30, -10,  20,  30,  30,  20, -10, -30],
                [-30, -30,   0,   0,   0,   0, -30, -30],
                [-50, -30, -30, -30, -30, -30, -30, -50]
            ])
        }
    }
}

impl Evaluator for StandardEvaluator {
    fn evaluate(&self, board: &Board, depth: u8) -> Evaluation {
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
            BoardStatus::Checkmate => Evaluation::mated_in(depth),
            BoardStatus::Stalemate => Evaluation::DRAW
        }
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
        let ally_pieces = *board.color_combined(side);

        for &piece in &ALL_PIECES {
            let pieces = ally_pieces & *board.pieces(piece);
            for square in pieces {
                value += match piece {
                    Piece::Pawn => self.pawn + self.pawn_table.get(side, square),
                    Piece::Knight => self.knight + self.knight_table.get(side, square),
                    Piece::Bishop => self.bishop + self.bishop_table.get(side, square),
                    Piece::Rook => self.rook + self.rook_table.get(side, square),
                    Piece::Queen => self.queen + self.queen_table.get(side, square),
                    Piece::King => {
                        let init = self.king_table.get(side, square);
                        let endgame = self.king_table_endgame.get(side, square);
                        let phase = phase as i32;
                        const MAX_PHASE: i32 = StandardEvaluator::MAX_PHASE as i32;
                        ((init * (MAX_PHASE - phase)) + (endgame * phase)) / MAX_PHASE
                    }
                };
            }
        }
        
        value
    }
}