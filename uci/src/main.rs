use std::io::{BufRead, Write};
use std::time::{Instant, Duration};
use std::sync::mpsc::{channel, TryRecvError};
use std::sync::Arc;

use chess::*;

use vampirc_uci::{UciInfoAttribute, UciMessage, UciOptionConfig, UciTimeControl};
use lunatic::*;
use lunatic::evaluation::{StandardEvaluator, EvaluationKind};
use lunatic::engine::SearchOptions;
use lunatic::oracle::Oracle;
use lunatic::time::{FixedTimeManager, StandardTimeManager, TimeManager};
use indexmap::IndexMap;

struct EngineSearch {
    time_manager: Box<dyn TimeManager>,
    start: Instant,
    time_left: Duration,
    search_stream: std::sync::mpsc::Receiver<ContextSearchResult>,
    search_request: SearchRequest
}

fn send_message(message: UciMessage) {
    println!("{}", message);
    std::io::stdout().flush().unwrap();
}

fn send_info(result: &ContextSearchResult) {
    let tt_filledness =
        result.result.transposition_table_entries
        * 1000
        / result.result.transposition_table_size;
    send_message(UciMessage::Info(vec![
        match result.result.value.kind() {
            EvaluationKind::Centipawn(cp) => UciInfoAttribute::from_centipawns(cp),
            EvaluationKind::MateIn(m) => UciInfoAttribute::from_mate(((m + 1) / 2) as i8),
            EvaluationKind::MatedIn(m) => UciInfoAttribute::from_mate(-(((m + 1) / 2) as i8))
        },
        UciInfoAttribute::Depth(result.result.depth as u8),
        UciInfoAttribute::Nodes(result.total_nodes_searched as u64),
        UciInfoAttribute::Pv(result.result.principal_variation.clone()),
        UciInfoAttribute::Time(vampirc_uci::Duration::from_std(result.total_search_duration).unwrap()),
        UciInfoAttribute::HashFull(tt_filledness as u16)
    ]));
}

struct UciOptions {
    transposition_table_size: usize,
    search_options: SearchOptions,
    percent_time_used_per_move: f32,
    minimum_time_used_per_move: Duration
}

fn main() {
    let mut position: Option<(Board, Vec<ChessMove>)> = None;
    let settings = LunaticContextSettings::<StandardEvaluator>::default();
    let engine = LunaticContext::new(settings);
    let mut search = None;

    const MEGABYTE: usize = 1000_000;
    //Use IndexMap to preserve options order
    let mut options_handlers: IndexMap<String, (UciOptionConfig, Box<dyn Fn(&mut UciOptions, String)>)>
        = IndexMap::new();
    let mut options = UciOptions {
        transposition_table_size: 4 * MEGABYTE,
        search_options: SearchOptions::default(),
        percent_time_used_per_move: 0.05f32,
        minimum_time_used_per_move: Duration::from_secs(0)
    };
    macro_rules! add_handlers {
        ($($option:expr => $handler:expr)*) => {
            $({
                let option = $option;
                options_handlers.insert(match &option {
                    UciOptionConfig::Check { name, .. } => name,
                    UciOptionConfig::Spin { name, .. } => name,
                    UciOptionConfig::Combo { name, .. } => name,
                    UciOptionConfig::Button { name } => name,
                    UciOptionConfig::String { name, .. } => name
                }.to_owned(), (option, Box::new($handler)));
            })*
        }
    }
    add_handlers! {
        UciOptionConfig::Spin {
            name: "Hash".to_owned(),
            default: Some((options.transposition_table_size / MEGABYTE) as i64),
            min: Some(0),
            max: Some(64 * 1000) //64 Gigabytes
        } => |options, value| {
            options.transposition_table_size = value
                .parse::<usize>()
                .unwrap()
                * MEGABYTE
        }
        UciOptionConfig::Spin {
            name: "Late Move Reduction".to_owned(),
            default: Some(options.search_options.late_move_reduction as i64),
            min: Some(0),
            max: Some(u8::MAX as i64)
        } => |options, value| {
            options.search_options.late_move_reduction = value
                .parse()
                .unwrap();
        }
        UciOptionConfig::Spin {
            name: "Late Move Leeway".to_owned(),
            default: Some(options.search_options.late_move_leeway as i64),
            min: Some(0),
            max: Some(u8::MAX as i64)
        } => |options, value| {
            options.search_options.late_move_leeway = value
                .parse()
                .unwrap();
        }
        UciOptionConfig::Check {
            name: "Null Move Pruning".to_owned(),
            default: Some(options.search_options.null_move_pruning)
        } => |options, value| {
            options.search_options.null_move_pruning = value
                .parse()
                .unwrap();
        }
        UciOptionConfig::Spin {
            name: "Null Move Reduction".to_owned(),
            default: Some(options.search_options.null_move_reduction as i64),
            min: Some(0),
            max: Some(u8::MAX as i64)
        } => |options, value| {
            options.search_options.null_move_reduction = value
                .parse()
                .unwrap();
        }
        UciOptionConfig::Spin {
            name: "Percent of time used per move".to_owned(),
            default: Some((options.percent_time_used_per_move * 100.0) as i64),
            min: Some(0),
            max: Some(100)
        } => |options, value| {
            options.percent_time_used_per_move = value
                .parse::<f32>()
                .unwrap()
                / 100f32;
        }
        UciOptionConfig::Spin {
            name: "Minimum time used per move (ms)".to_owned(),
            default: Some(options.minimum_time_used_per_move.as_secs() as i64),
            min: Some(0),
            max: Some(u32::MAX as i64)
        } => |options, value| {
            let time = value
                .parse()
                .unwrap();
            options.minimum_time_used_per_move =
                Duration::from_secs(time);
        }
    }

    let (messages_send, messages) = channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        while let Some(Ok(line)) = lines.next() {
            messages_send.send(vampirc_uci::parse_one(&line)).unwrap();
        }
    });
    'main: loop {
        let mut end_search = false;
        loop {
            match messages.try_recv() {
                Ok(message) => match message {
                    UciMessage::Uci => {
                        send_message(UciMessage::id_name("Lunatic"));
                        send_message(UciMessage::id_author("Analog Hors"));
                        for (option, _) in options_handlers.values() {
                            send_message(UciMessage::Option(option.clone()));
                        }
                        send_message(UciMessage::UciOk);
                    }
                    UciMessage::Debug(_) => {}
                    UciMessage::IsReady => send_message(UciMessage::ReadyOk),
                    UciMessage::SetOption { name, value } => {
                        if let Some((_, handler)) = options_handlers.get(&name) {
                            handler(&mut options, value.unwrap())
                        }
                    }
                    UciMessage::UciNewGame => {}
        
                    UciMessage::Position { fen, moves, .. } => {
                        let board = fen
                            .map(|fen| fen.as_str().parse().unwrap())
                            .unwrap_or_default();
                        position = Some((board, moves));
                    }
                    UciMessage::Go { time_control, search_control } => {
                        let time_manager: Box<dyn TimeManager>;
                        let mut max_depth = 64;
                        time_manager = match time_control {
                            Some(UciTimeControl::MoveTime(time)) => Box::new(
                                FixedTimeManager::new(time.to_std().unwrap())
                            ),
                            Some(UciTimeControl::TimeLeft {
                                white_time,
                                black_time,
                                ..
                            }) => {
                                let time_left = match position
                                    .as_ref()
                                    .unwrap()
                                    .0
                                    .side_to_move() {
                                    Color::White => white_time,
                                    Color::Black => black_time
                                }.unwrap().to_std().unwrap();
                                Box::new(StandardTimeManager::new(
                                    time_left, 
                                    options.percent_time_used_per_move,
                                    options.minimum_time_used_per_move
                                ))
                            }
                            Some(UciTimeControl::Ponder) => todo!(),
                            None => Box::new(
                                FixedTimeManager::new(Duration::from_secs(5))
                            ),
                            Some(UciTimeControl::Infinite) => Box::new(
                                FixedTimeManager::new(
                                    Duration::new(u64::MAX, 999_999_999)
                                )
                            )
                        };
                        
                        if let Some(search_control) = search_control {
                            if let Some(depth) = search_control.depth {
                                max_depth = depth;
                            }
                            //TODO implement the rest
                        }
                        let (initial_pos, moves) = position.take().unwrap();
                        let (info_channel, search_request) =
                            engine.begin_think(
                                initial_pos, 
                                moves,
                                options.transposition_table_size,
                                max_depth,
                                options.search_options.clone(),
                                Arc::new(Oracle)
                            );

                        search = Some(EngineSearch {
                            time_manager,
                            start: Instant::now(),
                            time_left: Duration::from_secs(u64::MAX),
                            search_stream: info_channel,
                            search_request
                        });
                    }
                    UciMessage::Stop => end_search = true,
                    
                    UciMessage::PonderHit => {}
                    UciMessage::Quit => break 'main,
                    UciMessage::Register { .. } => {}
                    UciMessage::Unknown(_, _) => {}
                    //Engine to GUI messages
                    _ => {}
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'main
            }
        }
        
        if let Some(s) = &mut search {
            if s.start.elapsed() > s.time_left {
                end_search = true;
            }
            if end_search {
                let mut search = search.take().unwrap();
                let mv = search.search_request
                    .terminate()
                    .unwrap()
                    .result
                    .mv;
                for result in search.search_stream.try_iter() {
                    send_info(&result);
                }
                send_message(UciMessage::best_move(mv));
            } else {
                for result in s.search_stream.try_iter() {
                    send_info(&result);
                    s.time_left = s.time_manager.update(result.result, result.search_duration);
                }
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
