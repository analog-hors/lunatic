use std::str::FromStr;

use serde::{Serialize, Deserialize};

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Eq, PartialEq)]
pub enum GameStatus {
    created,
    started,
    aborted,
    mate,
    resign,
    stalemate,
    timeout,
    draw,
    outOfTime,
    cheat,
    noStart,
    unknownFinish,
    variantEnd
}

impl GameStatus {
    pub fn ended(self) -> bool {
        self != GameStatus::created && self != GameStatus::started
    }
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize)]
pub enum GameWinner {
    black,
    white
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GameMessage {
    gameFull {
        state: GameState,
    },
    gameState {
        moves: String,
        status: GameStatus,
        winner: Option<GameWinner>
    },
    chatLine {
        
    }
}

#[derive(Serialize, Deserialize)]
pub struct GameState {
    pub moves: String,
    pub status: GameStatus,
    pub winner: Option<GameWinner>
}

pub fn parse_move(mv: &str) -> chess::ChessMove {
    let source = chess::Square::from_str(&mv[0..2]).unwrap();
    let dest = chess::Square::from_str(&mv[2..4]).unwrap();
    
    let promo = if mv.len() == 5 {
        Some(match mv.chars().last().unwrap() {
            'q' => chess::Piece::Queen,
            'r' => chess::Piece::Rook,
            'n' => chess::Piece::Knight,
            'b' => chess::Piece::Bishop,
            _ => panic!("Invalid promotion."),
        })
    } else {
        None
    };

    chess::ChessMove::new(source, dest, promo)
}
