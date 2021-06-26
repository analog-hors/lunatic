use chess::*;
use arraydeque::ArrayDeque;
use serde::{Serialize, Deserialize};

use crate::evaluator::*;
use crate::table::*;
use crate::moves::*;
use crate::oracle;

pub type HistoryTable = [[[u32; NUM_SQUARES]; NUM_PIECES]; NUM_COLORS];

pub(crate) type KillerTableEntry = ArrayDeque<[ChessMove; 2], arraydeque::Wrapping>;

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

pub trait LunaticHandler {
    fn time_up(&mut self) -> bool;

    fn search_result(&mut self, search_result: SearchResult);
}

pub struct LunaticSearchState<H> {
    handler: H,
    board: Board,
    history: Vec<u64>,
    halfmove_clock: u8,
    options: SearchOptions,
    cache_table: TranspositionTable,
    killer_table: Vec<KillerTableEntry>,
    history_table: HistoryTable
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

fn board_status(board: &Board, moves: &MoveGen) -> BoardStatus {
    if moves.len() > 0 {
        BoardStatus::Ongoing
    } else if *board.checkers() != EMPTY {
        BoardStatus::Checkmate
    } else {
        BoardStatus::Stalemate
    }
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
    pub null_move_reduction: u8,
    pub max_depth: u8,
    pub max_nodes: u32,
    pub transposition_table_size: usize
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            late_move_reduction: 1,
            late_move_leeway: 3,
            null_move_pruning: true,
            null_move_reduction: 2,
            max_depth: 64,
            max_nodes: u32::MAX,
            transposition_table_size: 16_000_000
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SearchError {
    MaxDepth,
    NoMoves,
    Terminated
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
        let halfmove_clock = history.len() as u8;

        Self {
            handler,
            board,
            history,
            halfmove_clock,
            cache_table: TranspositionTable::with_rounded_size(options.transposition_table_size),
            killer_table: vec![KillerTableEntry::new(); options.max_depth as usize],
            history_table: [[[0; NUM_SQUARES]; NUM_PIECES]; NUM_COLORS],
            options
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
                -Evaluation::INFINITY,
                Evaluation::INFINITY
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
        depth: u8,
        ply_index: u8,
        halfmove_clock: u8,
        mut alpha: Evaluation,
        mut beta: Evaluation
    ) -> Result<T::Output, ()> {
        if !T::REQUIRES_MOVE && *node_count % 4096 == 0 && self.handler.time_up() {
            return Err(());
        }

        *node_count += 1;

        if !T::REQUIRES_MOVE && draw_by_move_rule(board, &self.history, halfmove_clock) {
            return Ok(T::convert(|| Evaluation::DRAW, None));
        }

        let original_alpha = alpha;
        let moves = MoveGen::new_legal(&board);
        let status = board_status(board, &moves);
        if status != BoardStatus::Ongoing {
            let eval = if status == BoardStatus::Checkmate {
                Evaluation::mated_in(ply_index)
            } else {
                Evaluation::DRAW
            };
            return Ok(T::convert(|| eval, None));
        }

        if !T::REQUIRES_MOVE {
            if let Some(eval) = oracle::oracle(board) {
                return Ok(T::convert(|| eval, None));
            }
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
            Ok(T::convert(
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
                    let narrowed_alpha = beta - Evaluation::from_centipawns(1);
                    self.history.push(child_board.get_hash());
                    let child_value = -self.search_position::<PositionEvaluation>(
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

        //The reason we are allowed to safely return this score
        //is the assumption that even though we only check captures,
        //at any point in the search there is at least one other
        //move that matches or is better than the value, so we didn't
        //*necessarily* have to play this line and it's *probably* at
        //least that value.
        let moves = MoveGen::new_legal(&board);
        match board_status(board, &moves) {
            BoardStatus::Checkmate => return Evaluation::mated_in(ply_index),
            BoardStatus::Stalemate => return Evaluation::DRAW,
            _ => {}
        }
        let mut value = EVALUATOR.evaluate(board);
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
