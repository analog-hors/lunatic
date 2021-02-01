use chess::*;
use serde::{Serialize, Deserialize};

use crate::evaluation::Evaluator;
use crate::table::*;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct SearchInfo {
    pub value: i32,
    pub nodes: u32
}

pub struct LunaticSearchState {
    transposition_table: TranspositionTable
}

pub(crate) fn move_zeroes(mv: ChessMove, board: &Board) -> bool {
    // The only capturing move that doesn't move to the captured piece's square
    // is en passant, which is a pawn move and zeroes anyway
    board.piece_on(mv.get_source()) == Some(Piece::Pawn) ||
    board.piece_on(mv.get_dest()).is_some()
}

impl LunaticSearchState {
    pub fn new() -> Self {
        Self {
            transposition_table: TranspositionTable::new()
        }
    }

    pub fn best_move(
        &mut self,
        evaluator: &impl Evaluator,
        board: &Board,
        game_history: &mut Vec<u64>,
        // Zeroing means a move that resets the 50 move rule counter and represents an irreversible move.
        depth_since_zeroing: u8,
        max_depth: u8
    ) -> Option<(ChessMove, SearchInfo)> {
        let mut nodes = 1;
        let mut best_move = None;
        let mut best_value = -i32::MAX;
        for mv in crate::moves::get_moves(&self.transposition_table, board) {
            let child_board = board.make_move_new(mv);
            let depth_since_zeroing = if move_zeroes(mv, board) {
                1
            } else {
                depth_since_zeroing + 1
            };
            game_history.push(child_board.get_hash());
            let value = -self.evaluate_position(
                evaluator,
                &child_board,
                game_history,
                &mut nodes,
                0,
                depth_since_zeroing,
                max_depth,
                -i32::MAX,
                -best_value
            );
            game_history.pop();
            if best_move.is_none() || value > best_value {
                best_move = Some(mv);
                best_value = value;
            }
        }
        best_move.map(|mv| (mv, SearchInfo {
            value: best_value,
            nodes
        }))
    }
    
    pub fn evaluate_position(
        &mut self,
        evaluator: &impl Evaluator,
        board: &Board,
        game_history: &mut Vec<u64>,
        node_count: &mut u32,
        depth: u8,
        depth_since_zeroing: u8,
        max_depth: u8,
        mut alpha: i32,
        mut beta: i32
    ) -> i32 {
        //Fifty move rule
        if depth_since_zeroing >= 100 {
            return 0;
        }

        //Threefold repetition
        if depth_since_zeroing >= 6 {
            let repetitions = game_history
                .iter()
                .rev()
                .take(depth_since_zeroing as usize)
                .step_by(2) // Every second ply so it's our turn
                .filter(|&&hash| hash == board.get_hash())
                .count();
            if repetitions >= 3 {
                return 0;
            }
        }

        let original_alpha = alpha;
        
        *node_count += 1;
        if let Some(entry) = self.transposition_table.get(&board) {
            if entry.depth < depth {
                match entry.kind {
                    TableEntryKind::Exact => return entry.value,
                    TableEntryKind::LowerBound => alpha = alpha.max(entry.value),
                    TableEntryKind::UpperBound => beta = beta.min(entry.value)
                }
                if alpha >= beta {
                    return entry.value;
                }
            }
        }
        if depth >= max_depth || board.status() != BoardStatus::Ongoing {
            evaluator.evaluate(board, depth)
        } else {
            let mut value = -i32::MAX;
            for mv in crate::moves::get_moves(&self.transposition_table, board) {
                let child_board = board.make_move_new(mv);
                let depth_since_zeroing = if move_zeroes(mv, board) {
                    1
                } else {
                    depth_since_zeroing + 1
                };
                game_history.push(child_board.get_hash());
                let child_value = -self.evaluate_position(
                    evaluator,
                    &child_board,
                    game_history,
                    node_count,
                    depth + 1,
                    depth_since_zeroing,
                    max_depth,
                    -beta,
                    -alpha
                );
                game_history.pop();
                value = value.max(child_value);
                alpha = alpha.max(value);
                if alpha >= beta {
                    break;
                }
            }
            self.transposition_table.set(
                &board,
                match value {
                    _ if value <= original_alpha => TableEntryKind::UpperBound,
                    _ if value >= beta => TableEntryKind::LowerBound,
                    _ => TableEntryKind::Exact
                },
                value,
                depth
            );
            value
        }
    }
}