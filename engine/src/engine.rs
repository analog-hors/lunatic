use chess::*;
use arraydeque::ArrayDeque;

use crate::evaluation::Evaluator;
use crate::table::*;
use crate::moves::SortedMoveGenerator;

#[derive(Debug, Clone)]
pub struct SearchInfo {
    pub value: i32,
    pub nodes: u32,
    pub depth: u8,
    pub principal_variation: Vec<ChessMove>
}

pub(crate) type KillerTableEntry = ArrayDeque<[ChessMove; 2], arraydeque::Wrapping>;
pub struct LunaticSearchState {
    transposition_table: TranspositionTable,
    killer_table: Vec<KillerTableEntry>,
}

pub(crate) fn move_zeroes(mv: ChessMove, board: &Board) -> bool {
    // The only capturing move that doesn't move to the captured piece's square
    // is en passant, which is a pawn move and zeroes anyway
    board.piece_on(mv.get_source()) == Some(Piece::Pawn) ||
    board.piece_on(mv.get_dest()).is_some()
}

pub(crate) fn move_is_quiet(board: &Board, child_board: &Board) -> bool {
    //No captures or promotions
    child_board.combined().popcnt() == board.combined().popcnt() &&
    child_board.pieces(Piece::Pawn).popcnt() == board.pieces(Piece::Pawn).popcnt()
}

fn draw_by_move_rule(board: &Board, game_history: &[u64], depth_since_zeroing: u8) -> bool {
    //Fifty move rule
    if depth_since_zeroing >= 100 {
        return true;
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
            return true;
        }
    }
    
    false
}

trait SearchReturnType {
    type Output;

    fn convert(
        get_value: impl FnOnce() -> i32,
        mv: Option<ChessMove>
    ) -> Self::Output;
}

struct BestMove;

impl SearchReturnType for BestMove {
    type Output = Option<(ChessMove, i32)>;

    fn convert(get_value: impl FnOnce() -> i32, mv: Option<ChessMove>) -> Self::Output {
        mv.map(|mv| (mv, get_value()))
    }
}

struct PositionEvaluation;

impl SearchReturnType for PositionEvaluation {
    type Output = i32;

    fn convert(get_value: impl FnOnce() -> i32, _: Option<ChessMove>) -> Self::Output {
        get_value()
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
        // Zeroing means a move that resets the 50 move rule counter and represents an irreversible move.
        depth_since_zeroing: u8,
        max_depth: u8
    ) -> Option<(ChessMove, SearchInfo)> {
        let mut nodes = 0;
        self.search_position::<BestMove, E>(
                evaluator,
                board,
                game_history,
                &mut nodes,
                0,
                depth_since_zeroing,
                max_depth,
                -i32::MAX,
                i32::MAX
            )
            .map(|(mv, value)| {
                let mut principal_variation = Vec::new();
                let mut board = *board;
                let mut depth_since_zeroing = depth_since_zeroing;

                let mut next_move = Some(mv);
                while let Some(mv) = next_move.take() {
                    principal_variation.push(mv);
                    game_history.push(board.get_hash());
                    depth_since_zeroing = if move_zeroes(mv, &board) {
                        1
                    } else {
                        depth_since_zeroing + 1
                    };
                    board = board.make_move_new(mv);

                    next_move = if draw_by_move_rule(&board, game_history, depth_since_zeroing) {
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
                    depth: max_depth,
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
        depth_since_zeroing: u8,
        max_depth: u8,
        mut alpha: i32,
        mut beta: i32
    ) -> T::Output {
        *node_count += 1;

        if draw_by_move_rule(board, game_history, depth_since_zeroing) {
            return T::convert(|| 0, None);
        }

        let subtree_depth = max_depth - depth;
        let original_alpha = alpha;
        
        if let Some(entry) = self.transposition_table.get(&board) {
            //Larger subtree means deeper search
            if entry.subtree_depth >= subtree_depth {
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
        if depth >= max_depth || board.status() != BoardStatus::Ongoing {
            T::convert(
                || {
                    self.quiescence_search(
                        evaluator,
                        board,
                        game_history,
                        node_count,
                        depth,
                        depth_since_zeroing,
                        alpha,
                        beta
                    )
                }, 
                None
            )
        } else {
            let mut value = -i32::MAX;
            let mut best_move = None;
            let killer_move = self.killer_table[depth as usize].clone();
            for mv in SortedMoveGenerator::new(&self.transposition_table, killer_move, *board) {
                let child_board = board.make_move_new(mv);
                let depth_since_zeroing = if move_zeroes(mv, board) {
                    1
                } else {
                    depth_since_zeroing + 1
                };
                game_history.push(child_board.get_hash());
                let child_value = -self.search_position::<PositionEvaluation, E>(
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
                if child_value > value {
                    value = child_value;
                    best_move = Some(mv);
                }
                alpha = alpha.max(value);
                if alpha >= beta {
                    if move_is_quiet(&board, &child_board) {
                        self.killer_table[depth as usize].push_back(mv);
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
                    subtree_depth,
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
        depth: u8,
        depth_since_zeroing: u8,
        mut alpha: i32,
        beta: i32
    ) -> i32 {
        *node_count += 1;

        if draw_by_move_rule(board, game_history, depth_since_zeroing) {
            return 0;
        }

        //The reason we are allowed to safely return the alpha score
        //is the assumption that even though we only check captures,
        //at any point in the search there is at least one other
        //move that matches or is better than the value, so we didn't
        //*necessarily* have to play this line and it's *probably* at
        //least that value.
        let stand_pat = evaluator.evaluate(board, depth);
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
            let depth_since_zeroing = if move_zeroes(mv, board) {
                1
            } else {
                depth_since_zeroing + 1
            };
            game_history.push(child_board.get_hash());
            let child_value = -self.quiescence_search(
                evaluator,
                &child_board,
                game_history,
                node_count,
                depth + 1,
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
