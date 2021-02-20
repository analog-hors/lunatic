use chess::*;
use arraydeque::ArrayDeque;
use serde::{Serialize, Deserialize};

use crate::evaluation::{Evaluation, Evaluator};
use crate::table::*;
use crate::moves::SortedMoveGenerator;

#[derive(Debug, Clone)]
pub struct SearchInfo {
    pub value: Evaluation,
    pub nodes: u32,
    pub depth: u8,
    pub principal_variation: Vec<ChessMove>
}

pub(crate) type KillerTableEntry = ArrayDeque<[ChessMove; 2], arraydeque::Wrapping>;
pub struct LunaticSearchState {
    transposition_table: TranspositionTable,
    killer_table: Vec<KillerTableEntry>,
}

pub(crate) fn move_resets_fifty_move_rule(mv: ChessMove, board: &Board) -> bool {
    // The only capturing move that doesn't move to the captured piece's square
    // is en passant, which is a pawn move and zeroes anyway
    board.piece_on(mv.get_source()) == Some(Piece::Pawn) ||
    board.piece_on(mv.get_dest()).is_some()
}

///No captures or promotions
pub(crate) fn move_is_quiet(board: &Board, child_board: &Board) -> bool {
    child_board.combined().popcnt() == board.combined().popcnt() &&
    child_board.pieces(Piece::Pawn).popcnt() == board.pieces(Piece::Pawn).popcnt()
}

fn draw_by_move_rule(board: &Board, game_history: &[u64], halfmove_clock: u8) -> bool {
    //Fifty move rule
    if halfmove_clock >= 100 {
        return true;
    }

    //Threefold repetition
    if halfmove_clock >= 6 {
        let repetitions = game_history
            .iter()
            .rev()
            .take(halfmove_clock as usize)
            .step_by(2) // Every second ply so it's our turn
            .filter(|&&hash| hash == board.get_hash())
            .count();
        if repetitions >= 3 {
            return true;
        }
    }
    
    false
}

trait SearchReturnType {
    type Output;

    fn convert(
        get_value: impl FnOnce() -> Evaluation,
        mv: Option<ChessMove>
    ) -> Self::Output;
}

struct BestMove;

impl SearchReturnType for BestMove {
    type Output = Option<(ChessMove, Evaluation)>;

    fn convert(get_value: impl FnOnce() -> Evaluation, mv: Option<ChessMove>) -> Self::Output {
        mv.map(|mv| (mv, get_value()))
    }
}

struct PositionEvaluation;

impl SearchReturnType for PositionEvaluation {
    type Output = Evaluation;

    fn convert(get_value: impl FnOnce() -> Evaluation, _: Option<ChessMove>) -> Self::Output {
        get_value()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    ///How many plies the search is reduced by for a likely bad move
    pub late_move_reduction: u8,
    //TODO "late move leeway" is a pretty terrible identifier
    ///The number of moves explored before late move reduction kicks in
    pub late_move_leeway: u8
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            late_move_reduction: 1,
            late_move_leeway: 3
        }
    }
}

impl LunaticSearchState {
    pub fn new(transposition_table_size: usize, killer_table_size: usize) -> Self {
        Self {
            transposition_table: TranspositionTable::with_rounded_size(transposition_table_size),
            killer_table: vec![KillerTableEntry::new(); killer_table_size]
        }
    }

    pub fn best_move<E: Evaluator>(
        &mut self,
        evaluator: &E,
        board: &Board,
        game_history: &mut Vec<u64>,
        halfmove_clock: u8,
        depth: u8,
        options: &SearchOptions,
    ) -> Option<(ChessMove, SearchInfo)> {
        let mut nodes = 0;
        self.search_position::<BestMove, E>(
                evaluator,
                board,
                game_history,
                &mut nodes,
                depth,
                0,
                halfmove_clock,
                options,
                -Evaluation::INFINITY,
                Evaluation::INFINITY
            )
            .map(|(mv, value)| {
                let mut principal_variation = Vec::new();
                let mut board = *board;
                let mut halfmove_clock = halfmove_clock;

                let mut next_move = Some(mv);
                while let Some(mv) = next_move.take() {
                    principal_variation.push(mv);
                    game_history.push(board.get_hash());
                    halfmove_clock = if move_resets_fifty_move_rule(mv, &board) {
                        1
                    } else {
                        halfmove_clock + 1
                    };
                    board = board.make_move_new(mv);

                    next_move = if draw_by_move_rule(&board, game_history, halfmove_clock) {
                        None
                    } else {
                        self.transposition_table.get(&board).map(|e| e.best_move)
                    };
                }
                for _ in 0..principal_variation.len() {
                    game_history.pop();
                }
                
                let info = SearchInfo {
                    value,
                    nodes,
                    depth,
                    principal_variation
                };
                (mv, info)
            })
    }
    
    fn search_position<T: SearchReturnType, E: Evaluator>(
        &mut self,
        evaluator: &E,
        board: &Board,
        game_history: &mut Vec<u64>,
        node_count: &mut u32,
        depth: u8,
        ply_index: u8,
        halfmove_clock: u8,
        options: &SearchOptions,
        mut alpha: Evaluation,
        mut beta: Evaluation
    ) -> T::Output {
        *node_count += 1;

        if draw_by_move_rule(board, game_history, halfmove_clock) {
            return T::convert(|| Evaluation::DRAW, None);
        }

        let original_alpha = alpha;
        
        if let Some(entry) = self.transposition_table.get(&board) {
            //Larger subtree means deeper search
            if entry.depth >= depth {
                match entry.kind {
                    TableEntryKind::Exact => return T::convert(|| entry.value, Some(entry.best_move)),
                    TableEntryKind::LowerBound => alpha = alpha.max(entry.value),
                    TableEntryKind::UpperBound => beta = beta.min(entry.value)
                }
                if alpha >= beta {
                    return T::convert(|| entry.value, Some(entry.best_move));
                }
            }
        }
        if depth == 0 || board.status() != BoardStatus::Ongoing {
            T::convert(
                || {
                    self.quiescence_search(
                        evaluator,
                        board,
                        game_history,
                        node_count,
                        ply_index,
                        halfmove_clock,
                        alpha,
                        beta
                    )
                }, 
                None
            )
        } else {
            let mut value = -Evaluation::INFINITY;
            let mut best_move = None;
            let killer_move = self.killer_table[ply_index as usize].clone();
            let in_check = *board.checkers() != EMPTY;
            for (i, mv) in SortedMoveGenerator::new(&self.transposition_table, killer_move, *board).enumerate() {
                let child_board = board.make_move_new(mv);
                let quiet = move_is_quiet(&board, &child_board);
                let gives_check = *child_board.checkers() != EMPTY;
                let halfmove_clock = if move_resets_fifty_move_rule(mv, board) {
                    1
                } else {
                    halfmove_clock + 1
                };
                let mut reduced_depth = depth;
                if i as u8 > options.late_move_leeway && depth > 3 &&
                   quiet && !in_check && !gives_check {
                    reduced_depth = depth.saturating_sub(options.late_move_reduction);
                }
                game_history.push(child_board.get_hash());
                let mut child_value;
                loop {
                    child_value = -self.search_position::<PositionEvaluation, E>(
                        evaluator,
                        &child_board,
                        game_history,
                        node_count,
                        depth - 1,
                        ply_index + 1,
                        halfmove_clock,
                        options,
                        -beta,
                        -alpha
                    );
                    //If it was searched to a reduced depth and it
                    //increased alpha, search again with full depth
                    if reduced_depth < depth && child_value > alpha {
                        reduced_depth = depth;
                        continue;
                    }
                    break;
                }
                game_history.pop();
                if child_value > value || best_move.is_none() {
                    value = child_value;
                    best_move = Some(mv);
                }
                alpha = alpha.max(value);
                if alpha >= beta {
                    if quiet {
                        self.killer_table[ply_index as usize].push_back(mv);
                    }
                    break;
                }
            }
            let best_move = best_move.unwrap();
            self.transposition_table.set(
                &board,
                TableEntry {
                    kind: match value {
                        _ if value <= original_alpha => TableEntryKind::UpperBound,
                        _ if value >= beta => TableEntryKind::LowerBound,
                        _ => TableEntryKind::Exact
                    },
                    value,
                    depth,
                    best_move
                }
            );
            T::convert(|| value, Some(best_move))
        }
    }

    fn quiescence_search(
        &mut self,
        evaluator: &impl Evaluator,
        board: &Board,
        game_history: &mut Vec<u64>,
        node_count: &mut u32,
        ply_index: u8,
        halfmove_clock: u8,
        mut alpha: Evaluation,
        beta: Evaluation
    ) -> Evaluation {
        *node_count += 1;

        if draw_by_move_rule(board, game_history, halfmove_clock) {
            return Evaluation::DRAW;
        }

        //The reason we are allowed to safely return the alpha score
        //is the assumption that even though we only check captures,
        //at any point in the search there is at least one other
        //move that matches or is better than the value, so we didn't
        //*necessarily* have to play this line and it's *probably* at
        //least that value.
        let stand_pat = evaluator.evaluate(board, ply_index);
        if stand_pat > beta {
            return beta;
        }
        if alpha < stand_pat {
            alpha = stand_pat;
        }
        let mut captures = MoveGen::new_legal(board);
        //TODO excludes en-passant, does this matter?
        captures.set_iterator_mask(*board.combined());
        for mv in captures {
            let child_board = board.make_move_new(mv);
            let depth_since_zeroing = if move_resets_fifty_move_rule(mv, board) {
                1
            } else {
                halfmove_clock + 1
            };
            game_history.push(child_board.get_hash());
            let child_value = -self.quiescence_search(
                evaluator,
                &child_board,
                game_history,
                node_count,
                ply_index + 1,
                depth_since_zeroing,
                -beta,
                -alpha
            );
            game_history.pop();
            if child_value >= beta {
                return beta;
            }
            alpha = alpha.max(child_value);
        }
        alpha
    }
}
