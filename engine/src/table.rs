use chess::*;

use crate::evaluator::*;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TableEntryKind {
    Exact,
    LowerBound,
    UpperBound
}

#[derive(Debug, Copy, Clone)]
pub struct TableEntry {
    pub kind: TableEntryKind,
    pub value: Evaluation,
    ///Remaining depth to max depth (the size of the subtree)
    pub depth: u8,
    pub best_move: ChessMove
}

#[derive(Debug)]
pub struct TranspositionTable(Vec<Option<(u64, TableEntry)>>, usize);

//TODO consider using `unsafe` to speed up transposition table access by removing bounds checking?
impl TranspositionTable {
    ///Rounds up the number of entries to a power of two.
    ///`panic` on overflow.
    pub fn with_rounded_entries(entries: usize) -> Self {
        Self(vec![None; entries.checked_next_power_of_two().unwrap()], 0)
    }

    ///Converts the size in bytes to an amount of entries
    ///then rounds up the size to the nearest power of two.
    ///`panic` on overflow.
    pub fn with_rounded_size(size: usize) -> Self {
        Self::with_rounded_entries(size / std::mem::size_of::<TableEntry>())
    }

    pub fn get(&self, board: &Board) -> Option<TableEntry> {
        let hash = board.get_hash();
        let mask = self.0.len() - 1;
        if let Some((entry_hash, entry)) = self.0[hash as usize & mask] {
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
        let mask = self.0.len() - 1;
        let old = &mut self.0[hash as usize & mask];
        if let Some(old) = old {
            if old.0 == hash || entry.depth > old.1.depth {
                //Matching hashes uses the newer entry since it has more information.
                //Otherwise, select the deeper entry.
                *old = (hash, entry);
            }
        } else {
            //Insert to empty slot
            self.1 += 1;
            *old = Some((hash, entry));
        }
    }

    pub fn capacity(&self) -> usize {
        self.0.len()
    }

    pub fn len(&self) -> usize {
        self.1
    }
}