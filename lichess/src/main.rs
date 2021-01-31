use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::time::Duration;

use serde::{Serialize, Deserialize};
use futures_util::StreamExt;

use chess::*;

use lunatic::evaluation::StandardEvaluator;
use lunatic::*;

mod api;
use api::*;

const TOKEN: &str = "lunatic_lichess_token.txt";
const SETTINGS: &str = "lunatic_lichess_settings.yml";

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    api: String,
    think_time: u64,
    max_depth: u8,
    engine_settings: LunaticContextSettings<StandardEvaluator>,
    // opening_book: Option<String>
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api: "https://lichess.org".to_owned(),
            think_time: 10,
            max_depth: 64,
            engine_settings: LunaticContextSettings::default(),
            // opening_book: None
        }
    }
}

struct ChessSession {
    game_id: String,
    token: String,
    settings: Settings,
    engine: LunaticContext,
    client: reqwest::Client,
}

impl ChessSession {
    async fn run(&mut self) {
        let profile = self.client
            .get(&format!("{}/api/account", self.settings.api))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let profile: Profile = serde_json::from_str(&profile).unwrap();

        let mut stream = self.client
            .get(&format!("{}/api/bot/game/stream/{}", self.settings.api, self.game_id))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap()
            .bytes_stream();
        
        let mut position = Board::default();
        let mut color = ChessSide::White;
        let mut buffer = String::new();
        while let Some(Ok(bytes)) = stream.next().await {
            for byte in bytes {
                if byte as char == '\n' {
                    if buffer.is_empty() {
                        continue;
                    }
                    let state = match serde_json::from_str(&buffer).unwrap() {
                        GameMessage::GameFull { state, initial_fen, white, .. } => {
                            position = initial_fen;
                            color = if profile.id == white.id {
                                ChessSide::White
                            } else {
                                ChessSide::Black
                            };
                            Some((state.moves, state.status))
                        },
                        GameMessage::GameState { moves, status, .. } => Some((moves, status)),
                        _ => None
                    };
                    if let Some((moves, status)) = state {
                        if status.ended() {
                            return;
                        }
                        let turn = if moves.len() % 2 == 0 {
                            ChessSide::White
                        } else {
                            ChessSide::Black
                        };
                        if turn == color {
                            self.make_move(position, moves).await;
                        }
                    }
                    buffer.clear();
                } else {
                    buffer.push(byte as char);
                }
            }
        }
    }

    async fn make_move(&mut self, initial_pos: Board, moves: Vec<ChessMove>) {
        println!("Thinking. . .");
        self.engine.begin_think(initial_pos,  moves, self.settings.max_depth);
        tokio::time::delay_for(Duration::from_secs(self.settings.think_time)).await;
        let (mv, info) = self.engine.end_think().await.unwrap().unwrap();
        println!("Value: {}", info.value);
        println!("Nodes: {}", info.nodes);
        println!("Depth: {}", info.depth);
        println!("{}", mv);
        for _ in 0..10 {
            let result = self.client
                .post(&format!("{}/api/bot/game/{}/move/{}", self.settings.api, self.game_id, mv))
                .bearer_auth(&self.token)
                .send()
                .await;
            if result.is_ok() {
                return;
            }
        }
        panic!("Failed to send move.");
    }
}

#[tokio::main]
async fn main() {
    let game_id = if let Some(game_id) = std::env::args().skip(1).next() {
        game_id
    } else {
        eprintln!("No game ID argument.");
        return;
    };
    let token = match std::fs::read_to_string(TOKEN) {
        Ok(token) => token,
        Err(err) => {
            eprintln!("Failed to read {}: {}", TOKEN, err);
            return;
        }
    };
    let settings = match File::open(SETTINGS) {
        Ok(file) => match serde_yaml::from_reader(BufReader::new(file)) {
            Ok(settings) => settings,
            Err(err) => {
                eprintln!("Failed to parse {}: {}", SETTINGS, err);
                return;
            }
        },
        Err(err) => if err.kind() == std::io::ErrorKind::NotFound {
            match File::create(SETTINGS) {
                Ok(file) => {
                    let file = BufWriter::new(file);
                    let options = Settings::default();
                    if let Err(err) = serde_yaml::to_writer(file, &options) {
                        eprintln!("Failed to write to {}: {}", SETTINGS, err);
                        return;
                    } else {
                        options
                    }
                }
                Err(err) => {
                    eprintln!("Failed to create file {}: {}", SETTINGS, err);
                    return;
                }
            }
        } else {
            eprintln!("Failed to read {}: {}", SETTINGS, err);
            return;
        }
    };
    // let options = settings.engine_options.clone();
    // let opening_book = if let Some(path) = &settings.opening_book {
    //     match File::open(path) {
    //         Ok(book) => Some(book),
    //         Err(err) => {
    //             eprintln!("Failed to read opening book {}: {}", path, err);
    //             return;
    //         }
    //     }
    // } else {
    //     None
    // };
    // let engine = WaterBearInterface::new(board, evaluator, opening_book, options);
    let engine = LunaticContext::new(settings.engine_settings.clone());
    let client = reqwest::Client::new();
    
    ChessSession {
        game_id,
        token,
        settings,
        engine,
        client,
    }.run().await;
}
