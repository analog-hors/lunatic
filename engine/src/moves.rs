use std::cmp::Ordering;
use std::iter::Peekable;

use chess::*;

use crate::table::*;
use crate::engine::KillerTableEntry;

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

#[derive(Debug, PartialEq, Eq)]
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

pub struct SortedMoveGenerator {
    board: Board,
    pv_move: Option<ChessMove>,
    captures: Option<Peekable<MaxSelectionSorter<MvvLvaMove>>>,
    killer_move: KillerTableEntry,
    moves: MoveGen
}

impl Iterator for SortedMoveGenerator {
    type Item = ChessMove;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(mv) = self.pv_move.take() {
            self.moves.remove_move(mv);
            return Some(mv);
        }

        if self.captures.is_none() {
            let mut mvv_lva_moves = Vec::with_capacity(40);
            self.moves.set_iterator_mask(*self.board.combined());
            for mv in &mut self.moves {
                self.killer_move.retain(|&m| m == mv);
                let victim = self.board
                    .piece_on(mv.get_dest())
                    .unwrap_or(Piece::Pawn); // en passant
                let attacker = self.board
                    .piece_on(mv.get_source())
                    .unwrap();
                mvv_lva_moves.push(MvvLvaMove {
                    victim,
                    attacker,
                    mv
                });
            }
            self.moves.set_iterator_mask(!EMPTY);
            self.captures = Some(MaxSelectionSorter(mvv_lva_moves).peekable());
        }
        let captures = self.captures.as_mut().unwrap();
        
        if let Some(mv) = captures.peek() {
            //Wininng or equal capture
            if mv.victim >= mv.attacker {
                let mv = mv.mv;
                captures.next();
                return Some(mv);
            }
        }

        if let Some(mv) = self.killer_move.pop_front() {
            let mut moves = MoveGen::new_legal(&self.board);
            moves.set_iterator_mask(BitBoard::from_square(mv.get_dest()));
            for mv in moves {
                if mv.get_source() == mv.get_source() {
                    return Some(mv);
                }
            }
        }

        if let Some(mv) = self.moves.next() {
            //Quiet move
            return Some(mv);
        }

        //Losing capture
        captures.next().map(|mv| mv.mv)
    }
}

impl SortedMoveGenerator {
    pub fn new(table: &TranspositionTable, killer_move: KillerTableEntry, board: Board) -> Self {
        let pv_move = table.get(&board).map(|entry| entry.best_move);
        Self {
            board,
            pv_move,
            captures: None,
            killer_move,
            moves: MoveGen::new_legal(&board)
        }
    }
}

pub fn quiescence_move_generator(board: &Board) -> impl Iterator<Item=ChessMove> {
    //Chess branching factor is ~35
    let mut mvv_lva_moves = Vec::with_capacity(40);
    let mut captures = MoveGen::new_legal(board);
    //TODO excludes en-passant, does this matter?
    captures.set_iterator_mask(*board.combined());
    for mv in captures {
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
    MaxSelectionSorter(mvv_lva_moves).map(|mv| mv.mv)
}
