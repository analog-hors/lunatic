use serde::{Serialize, Deserialize};
use chess::*;

use crate::evaluation::Evaluator;

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
    pub queen_table: PieceSquareTable
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
            bishop: 3300,
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
            ])
        }
    }
}

impl Evaluator for StandardEvaluator {
    fn evaluate(&self, board: &chess::Board, depth: u8) -> i32 {
        match board.status() {
            BoardStatus::Ongoing => {
                let white = self.evaluate_for_side(board, chess::Color::White);
                let black = self.evaluate_for_side(board, chess::Color::Black);
                if board.side_to_move() == Color::White {
                    white - black
                } else {
                    black - white
                }
            },
            //Checkmate decays so that shorter mate sequences are valued over longer ones
            BoardStatus::Checkmate => -(i32::MAX - depth as i32),
            BoardStatus::Stalemate => 0
        }
    }
}

impl StandardEvaluator {
    fn evaluate_for_side(&self, board: &chess::Board, side: chess::Color) -> i32 {
        let mut value = 0;
        let ally_pieces = *board.color_combined(side);

        const PIECES: &[Piece] = &[
            Piece::Pawn,
            Piece::Knight,
            Piece::Bishop,
            Piece::Rook,
            Piece::Queen
        ];
        for &piece in PIECES {
            let pieces = ally_pieces & *board.pieces(piece);
            for square in pieces {
                value += match piece {
                    Piece::Pawn => self.pawn + self.pawn_table.get(side, square),
                    Piece::Knight => self.knight + self.knight_table.get(side, square),
                    Piece::Bishop => self.bishop + self.bishop_table.get(side, square),
                    Piece::Rook => self.rook + self.rook_table.get(side, square),
                    Piece::Queen => self.queen + self.queen_table.get(side, square),
                    _ => unreachable!()
                };
            }
        }
        
        value
    }
}