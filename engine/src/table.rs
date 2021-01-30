use chess::*;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TableEntryKind {
    Exact,
    LowerBound,
    UpperBound
}

#[derive(Debug, Copy, Clone)]
pub struct TableEntry {
    hash: u64,
    pub kind: TableEntryKind,
    pub value: i32,
    pub depth: u8
}

const TABLE_SIZE_POW_TWO_INDEX: usize = 15;
pub const TABLE_SIZE: usize = 1 << TABLE_SIZE_POW_TWO_INDEX;
const TABLE_INDEX_MASK: usize = TABLE_SIZE - 1;

#[derive(Debug)]
pub struct TranspositionTable([Option<TableEntry>; TABLE_SIZE]);

impl TranspositionTable {
    pub fn new() -> Self {
        Self([None; TABLE_SIZE])
    }

    pub fn get(&self, board: &Board) -> Option<TableEntry> {
        let hash = board.get_hash();
        if let Some(entry) = self.0[hash as usize & TABLE_INDEX_MASK] {
            if entry.hash == hash {
                return Some(entry);
            }
        }
        None
    }

    pub fn set(
        &mut self,
        board: &Board,
        kind: TableEntryKind,
        value: i32,
        depth: u8
    ) {
        let hash = board.get_hash();
        let entry = TableEntry {
            hash,
            kind,
            value,
            depth
        };
        let old = &mut self.0[hash as usize & TABLE_INDEX_MASK];
        if let Some(old) = old {
            if old.depth < entry.depth {
                *old = entry;
            }
        } else {
            *old = Some(entry);
        }
    }
}