use std::str::FromStr;

use chess::*;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
pub struct Profile {
    pub id: Option<String>
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GameStatus {
    Created,
    Started,
    Aborted,
    Mate,
    Resign,
    Stalemate,
    Timeout,
    Draw,
    OutOfTime,
    Cheat,
    NoStart,
    UnknownFinish,
    VariantEnd
}

impl GameStatus {
    pub fn ended(self) -> bool {
        self != GameStatus::Created && self != GameStatus::Started
    }
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChessSide {
    White,
    Black
}

#[derive(Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum GameMessage {
    GameFull {
        state: GameState,
        #[serde(rename = "initialFen")]
        #[serde(deserialize_with = "deserialize_board")]
        initial_fen: Board,
        white: Profile,
        black: Profile
    },
    GameState {
        #[serde(deserialize_with = "deserialize_moves")]
        moves: Vec<ChessMove>,
        status: GameStatus,
        winner: Option<ChessSide>
    },
    ChatLine {
        
    }
}

#[derive(Deserialize)]
pub struct GameState {
    #[serde(deserialize_with = "deserialize_moves")]
    pub moves: Vec<ChessMove>,
    pub status: GameStatus,
    pub winner: Option<ChessSide>
}

fn deserialize_board<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Board, D::Error> {
    let board: &str = Deserialize::deserialize(deserializer)?;
    Ok(board.parse().unwrap_or_default())
}

fn deserialize_moves<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<ChessMove>, D::Error> {
    let moves: &str = Deserialize::deserialize(deserializer)?;
    Ok(moves.split(' ').filter(|s| s.len() > 0).map(parse_move).collect())
}

fn parse_move(mv: &str) -> ChessMove {
    let source = Square::from_str(&mv[0..2]).unwrap();
    let dest = Square::from_str(&mv[2..4]).unwrap();
    
    let promo = if mv.len() == 5 {
        Some(match mv.chars().last().unwrap() {
            'q' => Piece::Queen,
            'r' => Piece::Rook,
            'n' => Piece::Knight,
            'b' => Piece::Bishop,
            _ => panic!("Invalid promotion."),
        })
    } else {
        None
    };

    ChessMove::new(source, dest, promo)
}
