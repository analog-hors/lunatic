use chess::*;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TableEntryKind {
    Exact,
    LowerBound,
    UpperBound
}

#[derive(Debug, Copy, Clone)]
pub struct TableEntry {
    pub kind: TableEntryKind,
    pub value: i32,
    ///Remaining depth to max depth (the size of the subtree)
    pub subtree_depth: u8,
    pub best_move: ChessMove
}

const TABLE_SIZE_POW_TWO_INDEX: usize = 14;
pub const TABLE_SIZE: usize = 1 << TABLE_SIZE_POW_TWO_INDEX;
const TABLE_INDEX_MASK: usize = TABLE_SIZE - 1;

#[derive(Debug)]
pub struct TranspositionTable([Option<(u64, TableEntry)>; TABLE_SIZE]);

impl TranspositionTable {
    pub fn new() -> Self {
        Self([None; TABLE_SIZE])
    }

    pub fn get(&self, board: &Board) -> Option<TableEntry> {
        let hash = board.get_hash();
        if let Some((entry_hash, entry)) = self.0[hash as usize & TABLE_INDEX_MASK] {
            if entry_hash == hash {
                return Some(entry);
            }
        }
        None
    }

    pub fn set(
        &mut self,
        board: &Board,
        entry: TableEntry
    ) {
        let hash = board.get_hash();
        let old = &mut self.0[hash as usize & TABLE_INDEX_MASK];
        if let Some(old) = old {
            if old.1.subtree_depth < entry.subtree_depth {
                *old = (hash, entry);
            }
        } else {
            *old = Some((hash, entry));
        }
    }
}