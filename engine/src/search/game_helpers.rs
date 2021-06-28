use chess::*;

pub fn move_resets_fifty_move_rule(mv: ChessMove, board: &Board) -> bool {
    // The only capturing move that doesn't move to the captured piece's square
    // is en passant, which is a pawn move and zeroes anyway
    board.pieces(Piece::Pawn) & BitBoard::from_square(mv.get_source()) |
    board.combined() & BitBoard::from_square(mv.get_dest()) != EMPTY
}

///No captures or promotions
pub fn move_is_quiet(board: &Board, child_board: &Board) -> bool {
    child_board.combined().popcnt() == board.combined().popcnt() &&
    child_board.pieces(Piece::Pawn).popcnt() == board.pieces(Piece::Pawn).popcnt()
}

pub fn board_status(board: &Board, moves: &MoveGen) -> BoardStatus {
    if moves.len() > 0 {
        BoardStatus::Ongoing
    } else if *board.checkers() != EMPTY {
        BoardStatus::Checkmate
    } else {
        BoardStatus::Stalemate
    }
}

pub fn draw_by_move_rule(board: &Board, game_history: &[u64], halfmove_clock: u8) -> bool {
    //Fifty move rule
    if halfmove_clock >= 100 {
        return true;
    }

    //Threefold repetition
    //Skip the first move (2 plies) and ensure at least one other move to compare it to (2 plies)
    if halfmove_clock >= 4 {
        //Any repetition means a loop where the best move involves repeating moves, so
        //the first repetition is immediately a draw. No point playing out three repetitions.

        let threefold = game_history
            .iter()
            .rev()
            .take(halfmove_clock as usize)
            .step_by(2) // Every second ply so it's our turn
            .skip(1) // Skip our board
            .any(|&hash| hash == board.get_hash());
        if threefold {
            return true;
        }
    }
    
    false
}
