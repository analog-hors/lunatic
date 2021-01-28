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
    pub queen_table: PieceSquareTable,
    pub king: i32,
    pub attacked_squares: i32,
    pub attacked_piece_multiplier: f32,
    pub defended_piece_multiplier: f32,
    pub defender_piece_multiplier: f32,
    pub enemy_king_mobility: i32,
    pub checkmate: i32,
    pub stalemate: i32
}

impl Default for StandardEvaluator {
    fn default() -> Self {
        Self {
            pawn: 1000,
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
            knight: 3200,
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
            rook: 5000,
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
            queen: 9000,
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
            king: 9000,
            attacked_squares: 100,
            attacked_piece_multiplier: 0.5,
            defended_piece_multiplier: 0.5,
            defender_piece_multiplier: -0.1,
            enemy_king_mobility: -50,
            checkmate: i32::MAX,
            stalemate: 0
        }
    }
}

impl Evaluator for StandardEvaluator {
    fn evaluate(&self, board: &chess::Board) -> i32 {
        let side_multiplier = if board.side_to_move() == Color::White {
            1
        } else {
            -1
        };
        let evaluation = match board.status() {
            chess::BoardStatus::Ongoing => {
                let white = self.evaluate_for_side(board, chess::Color::White);
                let black = self.evaluate_for_side(board, chess::Color::Black);
                white - black
            },
            // If it's checkmate/stalemate and it's a player's turn, they've actually lost/tied, not won.
            chess::BoardStatus::Checkmate => -self.checkmate,
            chess::BoardStatus::Stalemate => -self.stalemate
        };
        side_multiplier * evaluation
    }
}

impl StandardEvaluator {
    fn piece_value(&self, piece: chess::Piece, side: chess::Color, square: chess::Square) -> i32 {
        match piece {
            chess::Piece::Pawn => self.pawn + self.pawn_table.get(side, square),
            chess::Piece::Knight => self.knight + self.knight_table.get(side, square),
            chess::Piece::Bishop => self.bishop + self.bishop_table.get(side, square),
            chess::Piece::Rook => self.rook + self.rook_table.get(side, square),
            chess::Piece::Queen => self.queen + self.queen_table.get(side, square),
            chess::Piece::King => self.king,
        }
    }
    fn attacked_square(&self, board: &chess::Board, side: chess::Color, square: chess::Square, attacker: chess::Piece) -> i32 {
        let value = if let Some(piece) = board.piece_on(square) {
            let multiplier = if board.color_on(square).unwrap() == side {
                if piece == chess::Piece::King {
                    // Defending the king makes no sense
                    0.0
                } else {
                    self.defended_piece_multiplier
                }
            } else {
                self.attacked_piece_multiplier
            };
            (self.piece_value(piece, side, square) as f32 * multiplier) as i32
        } else {
            self.attacked_squares
        };
        value + (self.piece_value(attacker, side, square) as f32 * self.defended_piece_multiplier) as i32
    }
    fn evaluate_for_side(&self, board: &chess::Board, side: chess::Color) -> i32 {
        let mut value = 0;
        let all_pieces = *board.combined();
        let ally_pieces = *board.color_combined(side);
        let enemy_pieces = all_pieces ^ ally_pieces;
        let enemy_king_pos = (enemy_pieces & board.pieces(chess::Piece::King)).to_square();
        let mut enemy_king_moves = chess::get_king_moves(enemy_king_pos);
        enemy_king_moves &= !enemy_pieces;
        for square in ally_pieces & *board.pieces(chess::Piece::Pawn) {
            value += self.pawn;
            let moves = chess::get_pawn_attacks(square, side, !chess::EMPTY);
            enemy_king_moves &= !moves;
            for square in moves {
                value += self.attacked_square(board, side, square, chess::Piece::Pawn);
            }
        }
        for square in ally_pieces & *board.pieces(chess::Piece::Knight) {
            value += self.knight;
            let moves = chess::get_knight_moves(square);
            enemy_king_moves &= !moves;
            for square in moves {
                value += self.attacked_square(board, side, square, chess::Piece::Knight);
            }
        }
        for square in ally_pieces & *board.pieces(chess::Piece::Bishop) {
            value += self.bishop;
            let moves = chess::get_bishop_moves(square, all_pieces);
            enemy_king_moves &= !moves;
            for square in moves {
                value += self.attacked_square(board, side, square, chess::Piece::Bishop);
            }
        }
        for square in ally_pieces & *board.pieces(chess::Piece::Rook) {
            value += self.rook;
            let moves = chess::get_rook_moves(square, all_pieces);
            enemy_king_moves &= !moves;
            for square in moves {
                value += self.attacked_square(board, side, square, chess::Piece::Rook);
            }
        }
        for square in ally_pieces & *board.pieces(chess::Piece::Queen) {
            value += self.queen;
            let moves =
                chess::get_bishop_moves(square, all_pieces) | 
                chess::get_rook_moves(square, all_pieces);
            enemy_king_moves &= !moves;
            for square in moves {
                value += self.attacked_square(board, side, square, chess::Piece::Queen);
            }
        }
        value += enemy_king_moves.popcnt() as i32 * self.enemy_king_mobility;
        value
    }
}