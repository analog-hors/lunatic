use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::time::Duration;

use serde::{Serialize, Deserialize};
use futures_util::StreamExt;

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
    engine_settings: LunaticContextSettings<StandardEvaluator>,
    // opening_book: Option<String>
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api: "https://lichess.org".to_owned(),
            think_time: 10,
            engine_settings: LunaticContextSettings::default(),
            // opening_book: None
        }
    }
}

struct ChessSession {
    game_id: String,
    token: String,
    settings: Settings,
    board: chess::Board,
    engine: LunaticContext,
    client: reqwest::Client,
}

impl ChessSession {
    async fn run(&mut self) {
        let mut stream = self.client
            .get(&format!("{}/api/bot/game/stream/{}", self.settings.api, self.game_id))
            .bearer_auth(&self.token)
            .timeout(Duration::from_secs(60 * 60))
            .send()
            .await
            .unwrap()
            .bytes_stream();
            
        let mut skip_next = false;

        let mut buffer = String::new();
        while let Some(Ok(bytes)) = stream.next().await {
            for byte in bytes {
                if byte as char == '\n' {
                    if buffer.is_empty() {
                        continue;
                    }
                    match serde_json::from_str(&buffer).unwrap() {
                        GameMessage::gameFull { state } => {
                            if state.status.ended() {
                                return;
                            }
                            for mv in state.moves.split_ascii_whitespace() {
                                self.board = self.board.make_move_new(parse_move(mv));
                            }
                            self.next_move().await;
                            skip_next = true;
                        },
                        GameMessage::gameState { moves, status, .. } => {
                            if status.ended() {
                                return;
                            }
                            if !skip_next {
                                let mv = parse_move(moves.split_ascii_whitespace().last().unwrap());
                                self.board = self.board.make_move_new(mv);
                                self.next_move().await;
                            }
                            skip_next = !skip_next;
                        },
                        _ => {}
                    }
                    buffer.clear();
                } else {
                    buffer.push(byte as char);
                }
            }
        }
    }
    async fn next_move(&mut self) {
        println!("Thinking. . .");
        self.engine.begin_think(self.board);
        tokio::time::delay_for(Duration::from_secs(self.settings.think_time)).await;
        let (mv, info) = self.engine.end_think().await.unwrap().unwrap();
        println!("Value: {}", info.value);
        println!("Nodes: {}", info.nodes);
        println!("Depth: {}", info.depth);
        println!("{}", mv);
        self.board = self.board.make_move_new(mv);
        for _ in 0..10 {
            let result = self.client.post(&format!("{}/api/bot/game/{}/move/{}", self.settings.api, self.game_id, mv))
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
    let board = chess::Board::default();
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
        board,
        engine,
        client,
    }.run().await;
}
