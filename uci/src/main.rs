use std::io::{BufRead, Write};
use std::time::Instant;
use std::sync::mpsc::{channel, TryRecvError};

use chess::*;
use vampirc_uci::{Duration, UciInfoAttribute, UciMessage, UciOptionConfig, UciTimeControl};
use lunatic::*;
use lunatic::evaluation::{StandardEvaluator, EvaluationKind};

struct EngineSearch {
    start: Instant,
    think_time: Duration,
    info_channel: std::sync::mpsc::Receiver<SearchInfo>
}

impl EngineSearch {
    fn start(
        engine: &LunaticContext,
        initial_pos: Board,
        moves: Vec<ChessMove>,
        transposition_table_size: usize,
        max_depth: u8,
        think_time: Duration
    ) -> Self {
        let info_channel = engine.begin_think(initial_pos, moves, transposition_table_size, max_depth);
        Self {
            start: Instant::now(),
            think_time,
            info_channel
        }
    }

    fn think_time_elapsed(&self) -> bool {
        self.start.elapsed() >= self.think_time.to_std().unwrap()
    }

    fn finish(engine: &LunaticContext) -> Option<ChessMove> {
        futures::executor::block_on(engine.end_think()).unwrap()
    }
}

fn send_message(message: UciMessage) {
    println!("{}", message);
    std::io::stdout().flush().unwrap();
}

fn send_move(mv: ChessMove) {
    send_message(UciMessage::best_move(mv));
}

fn send_info(info: SearchInfo) {
    send_message(UciMessage::Info(vec![
        match info.value.kind() {
            EvaluationKind::Centipawn(cp) => UciInfoAttribute::from_centipawns(cp),
            EvaluationKind::MateIn(m) => UciInfoAttribute::from_mate(((m + 1) / 2) as i8),
            EvaluationKind::MatedIn(m) => UciInfoAttribute::from_mate(-(((m + 1) / 2) as i8))
        },
        UciInfoAttribute::Depth(info.depth as u8),
        UciInfoAttribute::Nodes(info.nodes as u64),
        UciInfoAttribute::Pv(info.principal_variation)
    ]));
}

fn main() {
    let mut position = None;
    let settings = LunaticContextSettings::<StandardEvaluator>::default();
    let engine = LunaticContext::new(settings);
    let mut search = None;
    const MEGABYTE: usize = 1000_000;
    let mut transposition_table_size = 4 * MEGABYTE;

    let (messages_send, messages) = channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        while let Some(Ok(line)) = lines.next() {
            messages_send.send(vampirc_uci::parse_one(&line)).unwrap();
        }
    });
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
                    },
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
                    let mut think_time = Duration::seconds(5);
                    let mut max_depth = 64;
                    if let Some(time_control) = time_control {
                        if let UciTimeControl::MoveTime(time) = time_control {
                            think_time = time;
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
                    search = Some(EngineSearch::start(
                        &engine,
                        initial_pos,
                        moves,
                        transposition_table_size,
                        max_depth,
                        think_time
                    ));
                }
                UciMessage::Stop => {
                    if let Some(search) = search.take() {
                        let mv = EngineSearch::finish(&engine).unwrap();
                        for search in search.info_channel.try_iter() {
                            send_info(search);
                        }
                        send_move(mv);
                    }
                }
                
                UciMessage::PonderHit => {}
                UciMessage::Quit => break,
                UciMessage::Register { .. } => {}
                UciMessage::Unknown(_, _) => {}
                //Engine to GUI messages
                _ => {}
            }
            Err(TryRecvError::Empty) => {},
            Err(TryRecvError::Disconnected) => break
        }
        if let Some(search) = &mut search {
            for search in search.info_channel.try_iter() {
                send_info(search);
            }
        }
        
        if let Some(s) = &search {
            if s.think_time_elapsed() {
                let mv = EngineSearch::finish(&engine).unwrap();
                for search in s.info_channel.try_iter() {
                    send_info(search);
                }
                send_move(mv);
                search = None;
            }
        }
    }
}
