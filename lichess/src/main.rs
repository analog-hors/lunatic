use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::time::{Instant, Duration};

use rand::prelude::*;
use rand::distributions::WeightedIndex;
use serde::{Serialize, Deserialize};
use futures_util::StreamExt;
use chess::*;

use chess_polyglot_reader::{PolyglotReader, PolyglotKey};

use lunatic::evaluation::StandardEvaluator;
use lunatic::engine::SearchOptions;
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
    transposition_table_size: usize,
    max_depth: u8,
    search_options: SearchOptions,
    engine_settings: LunaticContextSettings<StandardEvaluator>,
    opening_book: Option<String>,
    opening_book_weight_multiplier: u16
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api: "https://lichess.org".to_owned(),
            think_time: 5,
            transposition_table_size: 4_000_000,
            max_depth: 64,
            search_options: SearchOptions::default(),
            engine_settings: LunaticContextSettings::default(),
            opening_book: None,
            opening_book_weight_multiplier: 1
        }
    }
}

struct ChessSession {
    game_id: String,
    token: String,
    settings: Settings,
    engine: LunaticContext,
    client: reqwest::Client,
    opening_book: Option<PolyglotReader<File>>
}

enum ClientMoveInfo {
    Engine(Duration),
    Book(u16)
}

fn print_info(iter: impl Iterator<Item=SearchInfo>) {
    for info in iter {
        println!("Value: {}", info.value);
        println!("Depth: {}", info.depth);
        println!("Nodes: {}", info.nodes);
        print!("PV:");
        for mv in info.principal_variation {
            print!(" {}", mv);
        }
        println!();
    }
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
        let mut mv = None;
        if let Some(book) = &mut self.opening_book {
            let mut board = initial_pos;
            for &mv in &moves {
                board = board.make_move_new(mv);
            }
            let key = PolyglotKey::from_board(&board);
            let entries = book.get(&key).unwrap();
            let weights = entries
                .iter()
                .map(|e| e.weight * self.settings.opening_book_weight_multiplier);
            if let Ok(weights) = WeightedIndex::new(weights) {
                let mut entry = entries[weights.sample(&mut thread_rng())];
                if entry.mv.source.file == 4 && entry.mv.source.rank == entry.mv.dest.rank {
                    let is_castle = match (entry.mv.dest.file, entry.mv.dest.rank) {
                        (7, 0) => key.white_castle.king_side,
                        (0, 0) => key.white_castle.queen_side,
                        (7, 7) => key.black_castle.king_side,
                        (0, 7) => key.black_castle.queen_side,
                        _ => false
                    };
                    if is_castle {
                        if entry.mv.dest.file < entry.mv.source.file {
                            entry.mv.dest.file += 1;
                        } else {
                            entry.mv.dest.file -= 1;
                        }
                    }
                }
                mv = Some((entry.mv.into(), ClientMoveInfo::Book(entry.weight)));
            }
        }
        if mv.is_none() {
            let think_begin = Instant::now();
            let info_stream = self.engine.begin_think(
                initial_pos,
                moves,
                self.settings.transposition_table_size,
                self.settings.max_depth,
                self.settings.search_options.clone()
            );
            let now = Instant::now();
            while now.elapsed().as_secs() < self.settings.think_time {
                print_info(info_stream.try_iter());
            }
            let engine_mv = self.engine.end_think().await.unwrap().unwrap();
            print_info(info_stream.try_iter());
            mv = Some((engine_mv, ClientMoveInfo::Engine(think_begin.elapsed())));
        }
        let (mv, info) = mv.unwrap();
        println!("{}", mv);
        match info {
            ClientMoveInfo::Engine(think_time) => {
                let think_time = think_time.as_secs_f32();
                println!(
                    "Thought for {:.1} seconds (+{:.1} over target of {})",
                    think_time,
                    think_time - self.settings.think_time as f32,
                    self.settings.think_time
                );
            }
            ClientMoveInfo::Book(weight) => {
                println!("Picked book move.");
                println!("Weight: {}", weight);
            }
        }
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
    let opening_book = if let Some(path) = &settings.opening_book {
        match File::open(path) {
            Ok(book) => match PolyglotReader::new(book) {
                Ok(book) => Some(book),
                Err(err) => {
                    eprintln!("Failed to load opening book {}: {}", path, err);
                    return;
                }
            },
            Err(err) => {
                eprintln!("Failed to read opening book {}: {}", path, err);
                return;
            }
        }
    } else {
        None
    };
    let engine = LunaticContext::new(settings.engine_settings.clone());
    let client = reqwest::Client::new();
    ChessSession {
        game_id,
        token,
        settings,
        engine,
        client,
        opening_book,
    }.run().await;
}
