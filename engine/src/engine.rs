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

impl LunaticSearchState {
    pub fn new() -> Self {
        Self {
            transposition_table: TranspositionTable::new()
        }
    }

    fn get_moves(&self, board: &Board) -> impl Iterator<Item=ChessMove> {
        let mut pv_move = None;
        let mut pv_value = 0;
        for mv in MoveGen::new_legal(board) {
            let board = board.make_move_new(mv);
            if let Some(entry) = self.transposition_table.get(&board) {
                if entry.kind == TableEntryKind::Exact && (pv_move.is_none() || entry.value > pv_value) {
                    pv_move = Some(mv);
                    pv_value = entry.value;
                }
            }
        }
        
        let mut moves = MoveGen::new_legal(board);
        if let Some(mv) = pv_move {
            moves.remove_move(mv);
        }
        
        //Chess branching factor is said to be ~35
        let mut mvv_lva_moves = Vec::with_capacity(40);
        moves.set_iterator_mask(*board.combined());
        for mv in &mut moves {
            let victim = board
                .piece_on(mv.get_dest())
                .unwrap_or(Piece::Pawn); // en passant
            let attacker = board
                .piece_on(mv.get_source())
                .unwrap();
            mvv_lva_moves.push(((victim, attacker), mv));
        }
        moves.set_iterator_mask(!EMPTY);
        
        mvv_lva_moves.sort_unstable_by(|((v1, a1), _), ((v2, a2), _)| {
            //Most Valuable Victim, Least Valuable Aggressor
            v1.cmp(v2).then(a2.cmp(a1)).reverse()
        });
        
        pv_move
            .into_iter()
            .chain(mvv_lva_moves.into_iter().map(|(_, mv)| mv))
            .chain(moves)
    }

    pub fn best_move(
        &mut self,
        evaluator: &impl Evaluator,
        board: &Board,
        max_depth: u8
    ) -> Option<(ChessMove, SearchInfo)> {
        let mut nodes = 1;
        let mut best_move = None;
        let mut best_value = -i32::MAX;
        for mv in self.get_moves(board) {
            let child_board = board.make_move_new(mv);
            let value = -self.evaluate_position(
                evaluator,
                &child_board,
                &mut nodes,
                0,
                max_depth,
                -i32::MAX,
                -best_value
            );
            if best_move.is_none() || value > best_value {
                best_move = Some(mv);
                best_value = value;
            }
            if best_value >= i32::MAX {
                break;
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
        node_count: &mut u32,
        depth: u8,
        max_depth: u8,
        mut alpha: i32,
        mut beta: i32
    ) -> i32 {
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
            for mv in self.get_moves(board) {
                let child_board = board.make_move_new(mv);
                let child_value = -self.evaluate_position(
                    evaluator,
                    &child_board,
                    node_count,
                    depth + 1,
                    max_depth,
                    -beta,
                    -alpha
                );
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