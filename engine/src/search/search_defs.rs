use chess::*;
use serde::{Serialize, Deserialize};

use crate::evaluator::Eval;

pub trait LunaticHandler {
    fn time_up(&mut self) -> bool;

    fn search_result(&mut self, search_result: SearchResult);
}


#[derive(Debug, Clone)]
pub struct SearchResult {
    pub mv: ChessMove,
    pub value: Eval,
    pub nodes: u32,
    pub depth: u8,
    pub principal_variation: Vec<ChessMove>,
    pub transposition_table_size: usize,
    pub transposition_table_entries: usize
}

#[derive(Debug, Copy, Clone)]
pub enum SearchError {
    MaxDepth,
    NoMoves,
    Terminated
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
