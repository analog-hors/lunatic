use chess::*;

use crate::evaluation::Evaluation;

#[derive(Default)]
pub struct Oracle;

impl Oracle {
    pub fn new() -> Self {
        Self
    }

    pub fn eval(&self, board: &Board) -> Option<Evaluation> {
        let all_pieces = *board.combined();
        let bishops = *board.pieces(Piece::Bishop);
        let knights = *board.pieces(Piece::Knight);
        let kings = *board.pieces(Piece::King);

        match all_pieces.popcnt() {
            0 | 1 => unreachable!(),
            2 => Some(Evaluation::DRAW),
            3 => {
                //KBvK and KNvK is always a draw
                if bishops | knights != EMPTY {
                    Some(Evaluation::DRAW)
                } else {
                    None
                }
            }
            4 => {
                //KNvKN KNNvk. Always a draw except for a few positions that are mate in one.
                //All of those positions have a king on an edge and are incredibly rare,
                //so we just do a quick check for edge kings before returning a draw.
                if knights.popcnt() == 2 && (kings & EDGES) == EMPTY {
                    return Some(Evaluation::DRAW);
                }
                None
            }
            _ => None
        }
    }
}
