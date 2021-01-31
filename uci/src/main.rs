use std::io::{BufRead, Write};
use std::time::Instant;
use std::sync::mpsc::{channel, TryRecvError};

use chess::*;
use vampirc_uci::{UciMessage, UciTimeControl, Duration, UciInfoAttribute};
use lunatic::*;
use lunatic::MoveInfo;
use lunatic::evaluation::StandardEvaluator;

struct EngineSearch {
    start: Instant,
    think_time: Duration
}

impl EngineSearch {
    fn start(engine: &LunaticContext, initial_pos: Board, moves: Vec<ChessMove>, max_depth: u8, think_time: Duration) -> Self {
        engine.begin_think(initial_pos, moves, max_depth);
        Self {
            start: Instant::now(),
            think_time
        }
    }

    fn think_time_elapsed(&self) -> bool {
        self.start.elapsed() >= self.think_time.to_std().unwrap()
    }

    fn finish(engine: &LunaticContext) -> Option<(ChessMove, MoveInfo)> {
        futures::executor::block_on(engine.end_think()).unwrap()
    }
}

fn send_move(mv: ChessMove, info: MoveInfo) {
    send_message(UciMessage::best_move(mv));
    send_message(UciMessage::Info(vec![
        UciInfoAttribute::from_centipawns(info.value),
        UciInfoAttribute::Depth(info.depth as u8),
        UciInfoAttribute::Nodes(info.nodes as u64)
    ]));
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
                    send_message(UciMessage::UciOk);
                }
                UciMessage::Debug(_) => {}
                UciMessage::IsReady => send_message(UciMessage::ReadyOk),
                UciMessage::SetOption { .. } => {}
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
                        max_depth,
                        think_time
                    ));
                }
                UciMessage::Stop => {
                    if search.take().is_some() {
                        let (mv, info) = EngineSearch::finish(&engine).unwrap();
                        send_move(mv, info);
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
        if let Some(s) = &search {
            if s.think_time_elapsed() {
                let (mv, info) = EngineSearch::finish(&engine).unwrap();
                send_move(mv, info);
                search = None;
            }
        }
    }
}
