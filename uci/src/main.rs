use std::io::{BufRead, Write};
use std::time::{Instant, Duration};
use std::sync::mpsc::{channel, TryRecvError};

use vampirc_uci::{UciInfoAttribute, UciMessage, UciOptionConfig, UciTimeControl};
use lunatic::*;
use lunatic::evaluation::{StandardEvaluator, EvaluationKind};
use lunatic::engine::SearchOptions;

struct EngineSearch {
    start: Instant,
    think_time: Duration,
    info_channel: std::sync::mpsc::Receiver<(SearchInfo, Duration)>
}

fn send_message(message: UciMessage) {
    println!("{}", message);
    std::io::stdout().flush().unwrap();
}

fn main() {
    let mut position = None;
    let settings = LunaticContextSettings::<StandardEvaluator>::default();
    let engine = LunaticContext::new(settings);
    let mut search = None;
    const MEGABYTE: usize = 1000_000;
    let mut transposition_table_size = 4 * MEGABYTE;
    let mut search_options = SearchOptions::default();

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
                        send_message(UciMessage::Option(UciOptionConfig::Spin {
                            name: "Hash".to_owned(),
                            default: Some((transposition_table_size / MEGABYTE) as i64),
                            min: Some(0),
                            max: Some(64 * 1000) //64 Gigabytes
                        }));
                        send_message(UciMessage::Option(UciOptionConfig::Spin {
                            name: "Late Move Reduction".to_owned(),
                            default: Some(search_options.late_move_reduction as i64),
                            min: Some(0),
                            max: Some(u8::MAX as i64)
                        }));
                        send_message(UciMessage::Option(UciOptionConfig::Spin {
                            name: "Late Move Leeway".to_owned(),
                            default: Some(search_options.late_move_leeway as i64),
                            min: Some(0),
                            max: Some(u8::MAX as i64)
                        }));
                        send_message(UciMessage::Option(UciOptionConfig::Check {
                            name: "Null Move Pruning".to_owned(),
                            default: Some(search_options.null_move_pruning)
                        }));
                        send_message(UciMessage::Option(UciOptionConfig::Spin {
                            name: "Null Move Reduction".to_owned(),
                            default: Some(search_options.null_move_reduction as i64),
                            min: Some(0),
                            max: Some(u8::MAX as i64)
                        }));
                        send_message(UciMessage::UciOk);
                    }
                    UciMessage::Debug(_) => {}
                    UciMessage::IsReady => send_message(UciMessage::ReadyOk),
                    UciMessage::SetOption { name, value } => match &name[..] {
                        "Hash" => {
                            transposition_table_size = value
                                .unwrap()
                                .parse::<usize>()
                                .unwrap()
                                * MEGABYTE
                        }
                        "Late Move Reduction" => {
                            search_options.late_move_reduction = value
                                .unwrap()
                                .parse()
                                .unwrap();
                        }
                        "Late Move Leeway" => {
                            search_options.late_move_leeway = value
                                .unwrap()
                                .parse()
                                .unwrap();
                        }
                        "Null Move Pruning" => {
                            search_options.null_move_pruning = value
                                .unwrap()
                                .parse()
                                .unwrap();
                        }
                        "Null Move Reduction" => {
                            search_options.null_move_reduction = value
                                .unwrap()
                                .parse()
                                .unwrap();
                        }
                        _ => {}
                    }
                    UciMessage::UciNewGame => {}
        
                    UciMessage::Position { fen, moves, .. } => {
                        let board = fen
                            .map(|fen| fen.as_str().parse().unwrap())
                            .unwrap_or_default();
                        position = Some((board, moves));
                    }
                    UciMessage::Go { time_control, search_control } => {
                        let mut think_time = Duration::from_secs(5);
                        let mut max_depth = 64;
                        if let Some(time_control) = time_control {
                            if let UciTimeControl::MoveTime(time) = time_control {
                                think_time = time.to_std().unwrap();
                            }
                            //TODO implement the rest
                        }
                        if let Some(search_control) = search_control {
                            if let Some(depth) = search_control.depth {
                                max_depth = depth;
                            }
                            //TODO implement the rest
                        }
                        let (initial_pos, moves) = position.take().unwrap();
                        
                        search = Some(EngineSearch {
                            start: Instant::now(),
                            think_time,
                            info_channel: engine.begin_think(
                                initial_pos, 
                                moves,
                                transposition_table_size,
                                max_depth,
                                search_options.clone()
                            )
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
            let mut best_move = None;

            if s.start.elapsed() > s.think_time {
                end_search = true;
            }
            if end_search {
                let mv = futures::executor::block_on(engine.end_think())
                    .unwrap()
                    .unwrap();
                best_move = Some(mv);
            }

            for (info, time) in s.info_channel.try_iter() {
                send_message(UciMessage::Info(vec![
                    match info.value.kind() {
                        EvaluationKind::Centipawn(cp) => UciInfoAttribute::from_centipawns(cp),
                        EvaluationKind::MateIn(m) => UciInfoAttribute::from_mate(((m + 1) / 2) as i8),
                        EvaluationKind::MatedIn(m) => UciInfoAttribute::from_mate(-(((m + 1) / 2) as i8))
                    },
                    UciInfoAttribute::Depth(info.depth as u8),
                    UciInfoAttribute::Nodes(info.nodes as u64),
                    UciInfoAttribute::Pv(info.principal_variation),
                    UciInfoAttribute::Time(vampirc_uci::Duration::from_std(time).unwrap())
                ]));
            }
            if let Some(mv) = best_move {
                send_message(UciMessage::best_move(mv));
                search = None;
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
