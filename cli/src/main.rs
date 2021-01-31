use std::{str::FromStr, time::Duration};
use std::fs::File;
use std::io::{BufReader, BufWriter};

use chess::*;
use lunatic::*;
use lunatic::evaluation::StandardEvaluator;
use clap::{Arg, App};
use serde::{Serialize, Deserialize};

const SETTINGS: &str = "lunatic_cli_settings.yml";

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    think_time: u64,
    max_depth: u8,
    engine_settings: LunaticContextSettings<StandardEvaluator>
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            think_time: 10,
            max_depth: 5,
            engine_settings: LunaticContextSettings::default()
        }
    }
}

fn main() {
    let matches = App::new("Lunatic CLI")
        .arg(Arg::with_name("color")
            .short("c")
            .long("color")
            .value_name("COLOR")
            .help("The color Lunatic plays as")
            .takes_value(true)
            .required(true)
            .possible_values(&["white", "black"]))
        .arg(Arg::with_name("board")
            .short("b")
            .value_name("FEN STRING")
            .help("Initial FEN string")
            .takes_value(true)
            .validator(|s| {
                s.parse::<Board>()
                    .map(|_| ())
                    .map_err(|e| format!("{}", e))
            }))
        .arg(Arg::with_name("ndjson")
            .long("ndjson")
            .help("Switches to NDJSON communication (Meant for programmatic use)"))
        .get_matches();

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

    let engine_color = if matches.value_of("color").unwrap() == "white" {
        Color::White
    } else {
        Color::Black
    };
    let ndjson = matches.occurrences_of("ndjson") > 0;
    let mut board = matches
        .value_of("board")
        .map(|s| s.parse::<Board>().unwrap())
        .unwrap_or_default();
    let mut moves = Vec::new();
    
    let engine = LunaticContext::new(settings.engine_settings);
    loop {
        let mv = if board.side_to_move() == engine_color {
            engine.begin_think(board, moves.clone(), settings.max_depth);
            std::thread::sleep(Duration::from_secs(settings.think_time));
            if let Some((mv, info)) = futures::executor::block_on(engine.end_think()).unwrap() {
                if ndjson {
                    println!("{}", serde_json::to_string(&(mv.to_string(), info)).unwrap());
                } else {
                    println!("Value: {}", info.value);
                    println!("Nodes: {}", info.nodes);
                    println!("Depth: {}", info.depth);
                    println!("{}", mv);
                }
                mv
            } else {
                return
            }
        } else {
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();
            parse_move(if ndjson {
                serde_json::from_str(input).unwrap()
            } else {
                input
            })
        };
        moves.push(mv);
        board = board.make_move_new(mv);
    }
}

fn parse_move(mv: &str) -> chess::ChessMove {
    let source = chess::Square::from_str(&mv[0..2]).unwrap();
    let dest = chess::Square::from_str(&mv[2..4]).unwrap();
    
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
