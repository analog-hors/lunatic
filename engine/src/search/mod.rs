use chess::*;
use arraydeque::ArrayDeque;

use crate::evaluator::*;
use crate::table::*;
use crate::moves::*;
use crate::oracle;

mod game_helpers;
use game_helpers::*;

mod search_defs;
pub use search_defs::*;

trait SearchReturnType {
    type Output;
    const REQUIRES_MOVE: bool;

    fn convert(
        get_value: impl FnOnce() -> Eval,
        mv: Option<ChessMove>
    ) -> Self::Output;
}

struct BestMove;

impl SearchReturnType for BestMove {
    type Output = Option<(ChessMove, Eval)>;
    const REQUIRES_MOVE: bool = true;

    fn convert(get_value: impl FnOnce() -> Eval, mv: Option<ChessMove>) -> Self::Output {
        mv.map(|mv| (mv, get_value()))
    }
}

struct PosEval;

impl SearchReturnType for PosEval {
    type Output = Eval;
    const REQUIRES_MOVE: bool = false;

    fn convert(get_value: impl FnOnce() -> Eval, _: Option<ChessMove>) -> Self::Output {
        get_value()
    }
}

pub(crate) type HistoryTable = [[[u32; NUM_SQUARES]; NUM_PIECES]; NUM_COLORS];

pub(crate) type KillerTableEntry = ArrayDeque<[ChessMove; 2], arraydeque::Wrapping>;

pub struct LunaticSearchState<H> {
    handler: H,
    board: Board,
    history: Vec<u64>,
    halfmove_clock: u8,
    options: SearchOptions,
    cache_table: TranspositionTable,
    killer_table: Vec<KillerTableEntry>,
    history_table: HistoryTable,
    sel_depth: u8
}

impl<H: LunaticHandler> LunaticSearchState<H> {
    pub fn new(
        handler: H,
        init_pos: &Board,
        moves: impl IntoIterator<Item=ChessMove>,
        options: SearchOptions
    ) -> Self {
        //100 for history, +32 for quiescence search
        let mut history = Vec::with_capacity(100 + options.max_depth as usize + 32);
        let mut board = *init_pos;
        history.push(board.get_hash());
        for mv in moves {
            if move_resets_fifty_move_rule(mv, &board) {
                history.clear();
            }
            board = board.make_move_new(mv);
            history.push(board.get_hash());
        }
        let halfmove_clock = history.len() as u8 - 1;

        Self {
            handler,
            board,
            history,
            halfmove_clock,
            cache_table: TranspositionTable::with_rounded_size(options.transposition_table_size),
            killer_table: vec![KillerTableEntry::new(); options.max_depth as usize],
            history_table: [[[0; NUM_SQUARES]; NUM_PIECES]; NUM_COLORS],
            options,
            sel_depth: 0
        }
    }

    pub fn search(&mut self) {
        let history_len = self.history.len();

        let mut nodes = 0;
        for depth in 0..self.options.max_depth {
            let result = self.search_position::<BestMove>(
                &self.board.clone(),
                &mut nodes,
                depth,
                0,
                self.halfmove_clock,
                Eval::MIN,
                Eval::MAX
            );
            //Early termination may trash history, so restore the state.
            self.history.truncate(history_len);
            match result {
                Ok(Some((mv, value))) => {
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
                            self.cache_table.get(&board).map(|e| e.best_move)
                        };
                    }
                    self.history.truncate(history_len);
                    
                    self.handler.search_result(SearchResult {
                        mv,
                        value,
                        nodes,
                        depth,
                        sel_depth: self.sel_depth,
                        principal_variation,
                        transposition_table_size: self.cache_table.capacity(),
                        transposition_table_entries: self.cache_table.len(),
                    });
                },
                Ok(None) => {},
                Err(()) => break //Terminated
            }
        }
    }
    
    fn search_position<T: SearchReturnType>(
        &mut self,
        board: &Board,
        node_count: &mut u32,
        mut depth: u8,
        ply_index: u8,
        halfmove_clock: u8,
        mut alpha: Eval,
        mut beta: Eval
    ) -> Result<T::Output, ()> {
        self.sel_depth = self.sel_depth.max(ply_index);
        let original_alpha = alpha;

        if !T::REQUIRES_MOVE && *node_count % 4096 == 0 && self.handler.time_up() {
            return Err(());
        }

        *node_count += 1;

        if !T::REQUIRES_MOVE && draw_by_move_rule(board, &self.history, halfmove_clock) {
            return Ok(T::convert(|| Eval::DRAW, None));
        }

        let moves = MoveGen::new_legal(&board);
        let status = board_status(board, &moves);
        if status != BoardStatus::Ongoing {
            let eval = if status == BoardStatus::Checkmate {
                Eval::mated_in(ply_index)
            } else {
                Eval::DRAW
            };
            return Ok(T::convert(|| eval, None));
        }

        if !T::REQUIRES_MOVE {
            if let Some(eval) = oracle::oracle(board) {
                return Ok(T::convert(|| eval, None));
            }
        }

        let in_check = *board.checkers() != EMPTY;
        if in_check {
            //Check extensions.
            //Don't enter quiescence while in check.
            depth += 1;
        }

        if let Some(entry) = self.cache_table.get(&board) {
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

        if depth == 0 {
            return Ok(T::convert(
                || {
                    //Prevent double counting
                    *node_count -= 1;
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
        }

        let mut value = Eval::MIN;
        let mut best_move = None;
        let killers = self.killer_table[ply_index as usize].clone();
        let ally_pieces = *board.color_combined(board.side_to_move());
        let sliding_pieces = 
            *board.pieces(Piece::Rook) |
            *board.pieces(Piece::Bishop) |
            *board.pieces(Piece::Queen);

        //If I have at least one sliding piece...
        if self.options.null_move_pruning && ally_pieces & sliding_pieces != EMPTY {
            if let Some(child_board) = board.null_move() {
                let narrowed_alpha = beta - Eval::cp(1);
                self.history.push(child_board.get_hash());
                let child_value = -self.search_position::<PosEval>(
                    &child_board,
                    node_count,
                    depth.saturating_sub(self.options.null_move_reduction + 1),
                    ply_index + 1,
                    halfmove_clock + 1,
                    -beta,
                    -narrowed_alpha
                )?;
                self.history.pop();
                if child_value >= beta {
                    return Ok(T::convert(|| child_value, None));
                }
            }
        }
        let mut moves = SortedMoveGenerator::new(
            &self.cache_table,
            killers, 
            *board,
            moves
        );
        let mut index = 0;
        while let Some(mv) = moves.next(&self.history_table) {
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
            if index as u8 >= self.options.late_move_leeway && depth > 3 &&
                quiet && !in_check && !gives_check {
                reduced_depth = if self.options.late_move_reduction < depth {
                    depth - self.options.late_move_reduction
                } else {
                    1
                };
                narrowed_beta = alpha + Eval::cp(1);
            }
            self.history.push(child_board.get_hash());
            let mut child_value;
            loop {
                child_value = -self.search_position::<PosEval>(
                    &child_board,
                    node_count,
                    reduced_depth - 1,
                    ply_index + 1,
                    halfmove_clock,
                    -narrowed_beta,
                    -alpha
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
                    self.history_table
                        [board.side_to_move().to_index()]
                        [board.piece_on(mv.get_source()).unwrap().to_index()]
                        [mv.get_dest().to_index()]
                        += depth as u32 * depth as u32;
                }
                break;
            }
            index += 1;
        }
        let best_move = best_move.unwrap();
        self.cache_table.set(
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

    fn quiescence_search(
        &mut self,
        board: &Board,
        node_count: &mut u32,
        ply_index: u8,
        halfmove_clock: u8,
        mut alpha: Eval,
        mut beta: Eval
    ) -> Eval {
        *node_count += 1;

        if draw_by_move_rule(board, &self.history, halfmove_clock) {
            return Eval::DRAW;
        }

        if let Some(entry) = self.cache_table.get(&board) {
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


        let moves = MoveGen::new_legal(&board);
        match board_status(board, &moves) {
            BoardStatus::Checkmate => return Eval::mated_in(ply_index),
            BoardStatus::Stalemate => return Eval::DRAW,
            _ => {}
        }
        let mut value = EVALUATOR.evaluate(board);
        //The reason we are allowed to safely return this score
        //is the assumption that even though we only check captures,
        //at any point in the search there is at least one other
        //move that matches or is better than the value, so we didn't
        //*necessarily* have to play this line and it's *probably* at
        //least that value.
        if value > alpha {
            alpha = value;
            if alpha >= beta {
                return value;
            }
        }
        for mv in quiescence_move_generator(&board, moves) {
            let child_board = board.make_move_new(mv);
            let halfmove_clock = if move_resets_fifty_move_rule(mv, board) {
                1
            } else {
                halfmove_clock + 1
            };
            self.history.push(child_board.get_hash());
            let child_value = -self.quiescence_search(
                &child_board,
                node_count,
                ply_index + 1,
                halfmove_clock,
                -beta,
                -alpha
            );
            self.history.pop();
            if child_value > value {
                value = child_value;
                if value > alpha {
                    alpha = value;
                    if alpha >= beta {
                        return value;
                    }
                }
            }
        }
        value
    }
}
