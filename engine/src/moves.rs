use std::cmp::Ordering;
use std::iter::Peekable;

use arrayvec::ArrayVec;
use chess::*;

use crate::evaluation::*;
use crate::table::*;
use crate::engine::{HistoryTable, KillerTableEntry};

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

//TODO consider still using MVV-LVA for LxH captures as it's cheaper?
#[derive(Debug, PartialEq, Eq)]
struct SeeMove {
    value: Evaluation,
    mv: ChessMove
}

impl PartialOrd for SeeMove {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SeeMove {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

fn static_exchange_evaluation(evaluator: &impl Evaluator, board: &Board, capture: ChessMove) -> Evaluation {
    let color = board.side_to_move();
    let sq = capture.get_dest();

    let mut blockers = *board.combined();
    //Explicit types because rust-analyzer.
    //Terminology:
    //guard    - Any piece involved in the fight for the square
    //attacker - Piece who's side initiated the capture
    //defender - Piece who's side is defending the capture
    let guards_mask: BitBoard =
        get_king_moves(sq) & *board.pieces(Piece::King) |
        get_knight_moves(sq) & *board.pieces(Piece::Knight) |
        get_rook_moves(sq, blockers) & (*board.pieces(Piece::Rook) | *board.pieces(Piece::Queen)) |
        get_bishop_moves(sq, blockers) & (*board.pieces(Piece::Bishop) | *board.pieces(Piece::Queen));
    //Pawns are directional
    let attacker_mask: BitBoard = guards_mask |
        get_pawn_attacks(sq, !color, blockers) & *board.pieces(Piece::Pawn);
    let defender_mask: BitBoard = guards_mask |
        get_pawn_attacks(sq, color, blockers) & *board.pieces(Piece::Pawn);
    let mut attackers: BitBoard = *board.color_combined(color) & attacker_mask;
    let mut defenders: BitBoard = *board.color_combined(!color) & defender_mask;

    //A piece can be attacked at most 15 times, about double that accounting for defending it.
    //...I don't really want to figure out the exact value.
    let mut gains = ArrayVec::<[Evaluation; 32]>::new();
    let mut side_to_move = color;
    let mut square_piece_value = evaluator.piece_value(board.piece_on(sq).unwrap());
    let mut attacker_square = capture.get_source();
    loop {
        //Reverse the roles if our piece is being attacked.
        let (attackers, defenders) = if side_to_move == color {
            (&mut attackers, &mut defenders)
        } else {
            (&mut defenders, &mut attackers)
        };

        let attacker_bitboard = BitBoard::from_square(attacker_square);
        //Remove the attacker from the blockers and guards
        blockers ^= attacker_bitboard;
        *attackers ^= attacker_bitboard;
        //We may have exposed new guards. Only bishops, rooks and queens
        //can attack in this manner, so we only check those movesets.
        let guards_mask: BitBoard =
            get_rook_moves(sq, blockers) & (*board.pieces(Piece::Rook) | *board.pieces(Piece::Queen)) |
            get_bishop_moves(sq, blockers) & (*board.pieces(Piece::Bishop) | *board.pieces(Piece::Queen));
        *attackers |= *board.color_combined(side_to_move) & blockers & guards_mask;
        *defenders |= *board.color_combined(!side_to_move) & blockers & guards_mask;

        let attacker = board.piece_on(attacker_square).unwrap();
        let previous_score = gains.last().copied().unwrap_or_default();
        //Negamax: Our value is the inverse of our opponent's value.
        //Add the value of the piece on the square; We won that piece.
        gains.push(-previous_score + square_piece_value);

        //Now our attacker is on that square.
        square_piece_value = evaluator.piece_value(attacker);
        side_to_move = !side_to_move;
        if *defenders == EMPTY {
            //No one is left to defend.
            //Go back down the stack.
            while gains.len() > 1 {
                //Negamax. The null gain represents what happens if we just don't
                //continue capturing, and the gain represents what happens if we do.
                //Thus, we maximize the two, accounting for the fact that the null
                //gain is inverted because it's from the perspective of the opponent.
                let gain = gains.pop().unwrap();
                let null_gain = gains.last_mut().unwrap();
                *null_gain = -gain.max(-*null_gain);
            }
            return gains.pop().unwrap();
        }
        for &piece in &ALL_PIECES {
            let defenders: BitBoard = *defenders & *board.pieces(piece);
            if defenders != EMPTY {
                attacker_square = defenders.to_square();
                break;
            }
        }
    }
}

pub struct SortedMoveGenerator<'s, E> {
    evaluator: &'s E,
    board: Board,
    pv_move: Option<ChessMove>,
    captures: Option<Peekable<MaxSelectionSorter<SeeMove>>>,
    killers: KillerTableEntry,
    quiets: Option<Vec<ChessMove>>,
    moves: MoveGen
}

impl<'s, E: Evaluator> SortedMoveGenerator<'s, E> {
    pub fn new(
        table: &TranspositionTable,
        evaluator: &'s E,
        killers: KillerTableEntry,
        board: Board
    ) -> Self {
        let pv_move = table.get(&board).map(|entry| entry.best_move);
        Self {
            evaluator,
            board,
            pv_move,
            captures: None,
            killers,
            quiets: None,
            moves: MoveGen::new_legal(&board)
        }
    }

    pub fn next(&mut self, history_table: &HistoryTable) -> Option<ChessMove> {
        if let Some(mv) = self.pv_move.take() {
            self.moves.remove_move(mv);
            self.killers.retain(|&m| m != mv);
            return Some(mv);
        }

        if self.captures.is_none() {
            let mut see_moves = Vec::with_capacity(40);
            self.moves.set_iterator_mask(*self.board.combined());
            for mv in &mut self.moves {
                //Even though killers are quiet, it's possible the
                //same move is not quiet as it is a different position
                self.killers.retain(|&m| m != mv);
                let value = static_exchange_evaluation(
                    self.evaluator,
                    &self.board,
                    mv
                );
                see_moves.push(SeeMove {
                    value,
                    mv
                });
            }
            self.moves.set_iterator_mask(!EMPTY);
            self.captures = Some(MaxSelectionSorter(see_moves).peekable());
        }
        let captures = self.captures.as_mut().unwrap();
        
        if let Some(mv) = captures.peek() {
            //Wininng or equal capture
            if mv.value >= Evaluation::from_centipawns(0) {
                let mv = mv.mv;
                captures.next();
                return Some(mv);
            }
        }

        while let Some(mv) = self.killers.pop_front() {
            let mut moves = MoveGen::new_legal(&self.board);
            moves.set_iterator_mask(BitBoard::from_square(mv.get_dest()));
            for m in moves {
                if m.get_source() == mv.get_source() {
                    self.moves.remove_move(mv);
                    return Some(mv);
                }
            }
        }

        if self.quiets.is_none() {
            self.quiets = Some((&mut self.moves).collect());
        }
        let quiets = self.quiets.as_mut().unwrap();
        if !quiets.is_empty() {
            //Quiet move
            let board = &self.board;
            let index = quiets
                .iter()
                .enumerate()
                .max_by_key(|(_, mv)| {
                    history_table
                        [board.side_to_move().to_index()]
                        [board.piece_on(mv.get_source()).unwrap().to_index()]
                        [mv.get_dest().to_index()]
                })
                .unwrap()
                .0;
            return Some(quiets.swap_remove(index));
        }

        //Losing capture
        captures.next().map(|mv| mv.mv)
    }
}

pub fn quiescence_move_generator(evaluator: &impl Evaluator, board: &Board) -> impl Iterator<Item=ChessMove> {
    //Chess branching factor is ~35
    let mut see_moves = Vec::with_capacity(40);
    let mut captures = MoveGen::new_legal(board);
    //TODO excludes en-passant, does this matter?
    captures.set_iterator_mask(*board.combined());
    for mv in captures {
        let value = static_exchange_evaluation(
            evaluator,
            board,
            mv
        );
        see_moves.push(SeeMove {
            value,
            mv
        });
    }
    MaxSelectionSorter(see_moves).map(|mv| mv.mv)
}
