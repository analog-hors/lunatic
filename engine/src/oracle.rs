use chess::*;

use crate::evaluator::*;

pub fn oracle(board: &Board) -> Option<Eval> {
    let all_pieces = *board.combined();
    let white_pieces = *board.color_combined(Color::White);
    let bishops = *board.pieces(Piece::Bishop);
    let knights = *board.pieces(Piece::Knight);
    let kings = *board.pieces(Piece::King);

    match all_pieces.popcnt() {
        0 | 1 => unreachable!(),
        2 => Some(Eval::DRAW),
        3 => {
            //KBvK and KNvK is always a draw
            if bishops | knights != EMPTY {
                Some(Eval::DRAW)
            } else {
                None
            }
        }
        4 => {
            const fn dark_squares() -> BitBoard {
                let mut board: u64 = 1;
                while board.count_ones() < 32 {
                    board |= board << 2;
                }
                BitBoard(board)
            }
            const CORNERS: BitBoard = BitBoard(
                (1 << 1) | (1 << 7) | (1 << 56) | (1 << 63)
            );
            let one_piece_each = white_pieces.popcnt() == 2;

            //KNvKN KNNvk. Always a draw except for a few positions that are mate in one.
            //All of those positions have a king on an edge and are incredibly rare,
            //so we just do a quick check for edge kings before returning a draw.
            if knights.popcnt() == 2 && (kings & EDGES) == EMPTY {
                return Some(Eval::DRAW);
            }
            if bishops.popcnt() == 2 {
                if (bishops & dark_squares()).popcnt() != 1 {
                    //Both bishops are on the same color square
                    return Some(Eval::DRAW);
                }
                if one_piece_each && (kings & CORNERS) == EMPTY {
                    //Opposite color bishops. Check the corners
                    //since there's technically one checkmate.
                    return Some(Eval::DRAW);
                }
            }
            if knights.popcnt() == 1 && bishops.popcnt() == 1 {
                if one_piece_each && (kings & CORNERS) == EMPTY {
                    //Check the corners since there's technically one checkmate.
                    return Some(Eval::DRAW);
                }
            }
            None
        }
        _ => None
    }
}
