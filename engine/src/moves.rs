use std::cmp::Ordering;

use chess::*;

use crate::table::*;

struct MaxSelectionSorter<I>(Vec<I>);

impl<I: Ord> Iterator for MaxSelectionSorter<I> {
    type Item = I;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.0.is_empty() {
            let index = self.0
                .iter()
                .enumerate()
                .max_by_key(|e| e.1)
                .unwrap()
                .0;
            Some(self.0.swap_remove(index))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.0.len(), Some(self.0.len()))
    }
}
impl<I: Ord> ExactSizeIterator for MaxSelectionSorter<I> {}

#[derive(PartialEq, Eq)]
struct MvvLvaMove {
    victim: Piece,
    attacker: Piece,
    mv: ChessMove
}

impl PartialOrd for MvvLvaMove {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MvvLvaMove {
    fn cmp(&self, other: &Self) -> Ordering {
        //Most Valuable Victim, Least Valuable Aggressor
        self.victim.cmp(&other.victim).then(other.attacker.cmp(&self.attacker))
    }
}

pub fn get_moves(table: &TranspositionTable, board: &Board) -> impl Iterator<Item=ChessMove> {
    let mut pv_move = None;
    let mut pv_value = 0;
    for mv in MoveGen::new_legal(board) {
        let board = board.make_move_new(mv);
        if let Some(entry) = table.get(&board) {
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
        mvv_lva_moves.push(MvvLvaMove {
            victim,
            attacker,
            mv
        });
    }
    moves.set_iterator_mask(!EMPTY);
    let mvv_lva_moves = MaxSelectionSorter(mvv_lva_moves)
        .map(|mv| mv.mv);
    
    pv_move
        .into_iter()
        .chain(mvv_lva_moves)
        .chain(moves)
}
