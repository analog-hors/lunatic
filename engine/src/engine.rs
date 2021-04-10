use chess::*;
use arraydeque::ArrayDeque;
use serde::{Serialize, Deserialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::evaluation::{Evaluation, Evaluator};
use crate::oracle::Oracle;
use crate::table::*;
use crate::moves::*;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub mv: ChessMove,
    pub value: Evaluation,
    pub nodes: u32,
    pub depth: u8,
    pub principal_variation: Vec<ChessMove>,
    pub transposition_table_size: usize,
    pub transposition_table_entries: usize
}

pub(crate) type KillerTableEntry = ArrayDeque<[ChessMove; 2], arraydeque::Wrapping>;
pub struct LunaticSearchState<'s, E> {
    board: Board,
    evaluator: &'s E,
    history: Vec<u64>,
    halfmove_clock: u8,
    options: &'s SearchOptions,
    oracle: &'s Oracle,
    transposition_table: TranspositionTable,
    killer_table: Vec<KillerTableEntry>,
    current_depth: u8,
    max_depth: u8
}

pub(crate) fn move_resets_fifty_move_rule(mv: ChessMove, board: &Board) -> bool {
    // The only capturing move that doesn't move to the captured piece's square
    // is en passant, which is a pawn move and zeroes anyway
    board.pieces(Piece::Pawn) & BitBoard::from_square(mv.get_source()) |
    board.combined() & BitBoard::from_square(mv.get_dest()) != EMPTY
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
    //Skip the first move (2 plies) and ensure at least one other move to compare it to (2 plies)
    if halfmove_clock >= 4 {
        //Any repetition means a loop where the best move involves repeating moves, so
        //the first repetition is immediately a draw. No point playing out three repetitions.

        let threefold = game_history
            .iter()
            .rev()
            .take(halfmove_clock as usize)
            .step_by(2) // Every second ply so it's our turn
            .skip(1) // Skip our board
            .any(|&hash| hash == board.get_hash());
        if threefold {
            return true;
        }
    }
    
    false
}

trait SearchReturnType {
    type Output;
    const REQUIRES_MOVE: bool;

    fn convert(
        get_value: impl FnOnce() -> Evaluation,
        mv: Option<ChessMove>
    ) -> Self::Output;
}

struct BestMove;

impl SearchReturnType for BestMove {
    type Output = Option<(ChessMove, Evaluation)>;
    const REQUIRES_MOVE: bool = true;

    fn convert(get_value: impl FnOnce() -> Evaluation, mv: Option<ChessMove>) -> Self::Output {
        mv.map(|mv| (mv, get_value()))
    }
}

struct PositionEvaluation;

impl SearchReturnType for PositionEvaluation {
    type Output = Evaluation;
    const REQUIRES_MOVE: bool = false;

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
    pub late_move_leeway: u8,
    ///Enable null move pruning?
    pub null_move_pruning: bool,
    ///The number of plies the null move pruning search is reduced by
    pub null_move_reduction: u8
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            late_move_reduction: 1,
            late_move_leeway: 3,
            null_move_pruning: true,
            null_move_reduction: 2
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SearchError {
    MaxDepth,
    NoMoves,
    Terminated
}

impl<'s, E: Evaluator> LunaticSearchState<'s, E> {
    pub fn new(
        board: &Board,
        evaluator: &'s E,
        history: &[u64],
        halfmove_clock: u8,
        options: &'s SearchOptions,
        oracle: &'s Oracle,
        transposition_table_size: usize,
        max_depth: u8
    ) -> Self {
        let mut history = history.to_vec();
        //+ 32 for quiescence search
        history.reserve_exact(max_depth as usize + 32);
        Self {
            board: *board,
            evaluator,
            history,
            halfmove_clock,
            options,
            oracle,
            transposition_table: TranspositionTable::with_rounded_size(transposition_table_size),
            killer_table: vec![KillerTableEntry::new(); max_depth as usize],
            current_depth: 0,
            max_depth
        }
    }

    pub fn deepen(&mut self, terminator: &Arc<AtomicBool>) -> Result<SearchResult, SearchError> {
        if self.current_depth >= self.max_depth {
            return Err(SearchError::MaxDepth);
        }

        let history_len = self.history.len();

        let mut nodes = 0;
        self.current_depth += 1;
        let result = self.search_position::<BestMove>(
            &self.board.clone(),
            &mut nodes,
            self.current_depth,
            0,
            self.halfmove_clock,
            -Evaluation::INFINITY,
            Evaluation::INFINITY,
            terminator
        );
        //Early termination may trash history, so restore the state.
        self.history.truncate(history_len);
        
        match result {
            Ok(Some((mv, value))) => Ok({
                let mut principal_variation = Vec::new();
                let mut board = self.board;
                let mut halfmove_clock = self.halfmove_clock;

                let mut next_move = Some(mv);
                while let Some(mv) = next_move.take() {
                    halfmove_clock = if move_resets_fifty_move_rule(mv, &board) {
                        1
                    } else {
                        halfmove_clock + 1
                    };
                    board = board.make_move_new(mv);
                    principal_variation.push(mv);
                    self.history.push(board.get_hash());

                    next_move = if draw_by_move_rule(&board, &self.history, halfmove_clock) {
                        None
                    } else {
                        self.transposition_table.get(&board).map(|e| e.best_move)
                    };
                }
                self.history.truncate(history_len);
                
                SearchResult {
                    mv,
                    value,
                    nodes,
                    depth: self.current_depth,
                    principal_variation,
                    transposition_table_size: self.transposition_table.capacity(),
                    transposition_table_entries: self.transposition_table.len(),
                }
            }),
            Ok(None) => Err(SearchError::NoMoves),
            Err(()) => Err(SearchError::Terminated)
        }
    }
    
    fn search_position<T: SearchReturnType>(
        &mut self,
        board: &Board,
        node_count: &mut u32,
        depth: u8,
        ply_index: u8,
        halfmove_clock: u8,
        mut alpha: Evaluation,
        mut beta: Evaluation,
        terminator: &Arc<AtomicBool>
    ) -> Result<T::Output, ()> {
        if terminator.load(Ordering::Acquire) {
            return Err(());
        }

        *node_count += 1;

        if draw_by_move_rule(board, &self.history, halfmove_clock) {
            return Ok(T::convert(|| Evaluation::DRAW, None));
        }

        let original_alpha = alpha;
        let terminal_node = board.status() != BoardStatus::Ongoing;
        if !terminal_node && !T::REQUIRES_MOVE {
            if let Some(eval) = self.oracle.eval(board) {
                return Ok(T::convert(|| eval, None));
            }
        }

        if let Some(entry) = self.transposition_table.get(&board) {
            //Larger subtree means deeper search
            if entry.depth >= depth {
                match entry.kind {
                    TableEntryKind::Exact => return Ok(T::convert(|| entry.value, Some(entry.best_move))),
                    TableEntryKind::LowerBound => alpha = alpha.max(entry.value),
                    TableEntryKind::UpperBound => beta = beta.min(entry.value)
                }
                if alpha >= beta {
                    return Ok(T::convert(|| entry.value, Some(entry.best_move)));
                }
            }
        }
        if depth == 0 || terminal_node {
            Ok(T::convert(
                || {
                    self.quiescence_search(
                        board,
                        node_count,
                        ply_index,
                        halfmove_clock,
                        alpha,
                        beta
                    )
                }, 
                None
            ))
        } else {
            let mut value = -Evaluation::INFINITY;
            let mut best_move = None;
            let killers = self.killer_table[ply_index as usize].clone();
            let in_check = *board.checkers() != EMPTY;
            let ally_pieces = *board.color_combined(board.side_to_move());
            let sliding_pieces = 
                *board.pieces(Piece::Rook) |
                *board.pieces(Piece::Bishop) |
                *board.pieces(Piece::Queen);

            //If I have at least one sliding piece...
            if self.options.null_move_pruning && ally_pieces & sliding_pieces != EMPTY {
                if let Some(child_board) = board.null_move() {
                    self.history.push(child_board.get_hash());
                    let child_value = -self.search_position::<PositionEvaluation>(
                        &child_board,
                        node_count,
                        depth.saturating_sub(self.options.null_move_reduction),
                        ply_index + 1,
                        halfmove_clock + 1,
                        -beta,
                        -alpha,
                        terminator
                    )?;
                    self.history.pop();
                    if child_value >= beta {
                        return Ok(T::convert(|| beta, None));
                    }
                }
            }
            for (i, mv) in SortedMoveGenerator::new(
                &self.transposition_table,
                self.evaluator,
                killers, 
                *board
            ).enumerate() {
                let child_board = board.make_move_new(mv);
                let quiet = move_is_quiet(&board, &child_board);
                let gives_check = *child_board.checkers() != EMPTY;
                let halfmove_clock = if move_resets_fifty_move_rule(mv, board) {
                    1
                } else {
                    halfmove_clock + 1
                };
                let mut reduced_depth = depth;
                let mut narrowed_beta = beta;
                if i as u8 > self.options.late_move_leeway && depth > 3 &&
                   quiet && !in_check && !gives_check {
                    reduced_depth = if self.options.late_move_reduction < depth {
                        depth - self.options.late_move_reduction
                    } else {
                        1
                    };
                    narrowed_beta = alpha + Evaluation::from_centipawns(1);
                }
                self.history.push(child_board.get_hash());
                let mut child_value;
                loop {
                    child_value = -self.search_position::<PositionEvaluation>(
                        &child_board,
                        node_count,
                        reduced_depth - 1,
                        ply_index + 1,
                        halfmove_clock,
                        -narrowed_beta,
                        -alpha,
                        terminator
                    )?;

                    //If it was searched to a reduced depth and it
                    //increased alpha, search again with full depth
                    if reduced_depth < depth && child_value > alpha {
                        reduced_depth = depth;
                        narrowed_beta = beta;
                        continue;
                    }
                    break;
                }
                self.history.pop();
                if child_value > value || best_move.is_none() {
                    value = child_value;
                    best_move = Some(mv);
                }
                alpha = alpha.max(value);
                if alpha >= beta {
                    if quiet {
                        let entry = &mut self.killer_table[ply_index as usize];
                        entry.retain(|&m| m != mv);
                        entry.push_back(mv);
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
            Ok(T::convert(|| value, Some(best_move)))
        }
    }

    fn quiescence_search(
        &mut self,
        board: &Board,
        node_count: &mut u32,
        ply_index: u8,
        halfmove_clock: u8,
        mut alpha: Evaluation,
        mut beta: Evaluation
    ) -> Evaluation {
        *node_count += 1;

        if draw_by_move_rule(board, &self.history, halfmove_clock) {
            return Evaluation::DRAW;
        }

        if let Some(entry) = self.transposition_table.get(&board) {
            //Literally any hit is better than quiescence search
            match entry.kind {
                TableEntryKind::Exact => return entry.value,
                TableEntryKind::LowerBound => alpha = alpha.max(entry.value),
                TableEntryKind::UpperBound => beta = beta.min(entry.value),
            }
            if alpha >= beta {
                return entry.value;
            }
        }

        //The reason we are allowed to safely return the alpha score
        //is the assumption that even though we only check captures,
        //at any point in the search there is at least one other
        //move that matches or is better than the value, so we didn't
        //*necessarily* have to play this line and it's *probably* at
        //least that value.
        let stand_pat = self.evaluator.evaluate(board, ply_index);
        if stand_pat > beta {
            return beta;
        }
        if alpha < stand_pat {
            alpha = stand_pat;
        }
        for mv in quiescence_move_generator(self.evaluator, &board) {
            let child_board = board.make_move_new(mv);
            let depth_since_zeroing = if move_resets_fifty_move_rule(mv, board) {
                1
            } else {
                halfmove_clock + 1
            };
            self.history.push(child_board.get_hash());
            let child_value = -self.quiescence_search(
                &child_board,
                node_count,
                ply_index + 1,
                depth_since_zeroing,
                -beta,
                -alpha
            );
            self.history.pop();
            if child_value >= beta {
                return beta;
            }
            alpha = alpha.max(child_value);
        }
        alpha
    }
}
