use std::io::{BufRead, BufReader, Write, stdin};
use std::time::{Instant, Duration};
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender};
use std::sync::atomic::{AtomicBool, Ordering};

use chess::*;

use vampirc_uci::{UciInfoAttribute, UciMessage, UciOptionConfig, UciTimeControl};
use lunatic::evaluator::*;
use lunatic::engine::*;
use lunatic::time::*;
use indexmap::IndexMap;

struct UciHandler {
    time_manager: StandardTimeManager,
    search_begin: Instant,
    last_update: Instant,
    time_left: Duration,
    search_terminator: Arc<AtomicBool>,
    event_sink: Sender<Event>,
    prev_result: Option<SearchResult>
}

impl LunaticHandler for &mut UciHandler {
    fn time_up(&mut self) -> bool {
        self.time_left < self.last_update.elapsed() ||
        self.search_terminator.load(Ordering::Acquire)
    }

    fn search_result(&mut self, result: SearchResult) {
        self.time_left = self.time_manager.update(result.clone(), self.last_update.elapsed());
        self.last_update = Instant::now();
        self.prev_result = Some(result.clone());
        self.event_sink.send(
            Event::EngineSearchUpdate(
                EngineSearchResult::SearchInfo(
                    result,
                    self.search_begin.elapsed()
                )
            )
        ).unwrap();
    }
}

impl UciHandler {
    fn finish(mut self) {
        self.event_sink.send(
            Event::EngineSearchUpdate(
                EngineSearchResult::SearchFinished(
                    self.prev_result.take().unwrap()
                )
            )
        ).unwrap();
    }
}

enum EngineSearchResult {
    SearchInfo(SearchResult, Duration),
    SearchFinished(SearchResult)
}

fn send_message(message: UciMessage) {
    println!("{}", message);
    std::io::stdout().flush().unwrap();
}

struct UciOptions {
    transposition_table_size: usize,
    search_options: SearchOptions,
    percent_time_used_per_move: f32,
    minimum_time_used_per_move: Duration
}

enum Event {
    UciMessage(UciMessage),
    EngineSearchUpdate(EngineSearchResult)
}

fn main() {
    let mut position: Option<(Board, Vec<ChessMove>)> = None;
    let mut search = None;

    const MEGABYTE: usize = 1000_000;
    //Use IndexMap to preserve options order
    let mut options_handlers: IndexMap<String, (UciOptionConfig, Box<dyn Fn(&mut UciOptions, String)>)>
        = IndexMap::new();
    let mut options = UciOptions {
        transposition_table_size: 4 * MEGABYTE,
        search_options: SearchOptions::default(),
        percent_time_used_per_move: 0.05f32,
        minimum_time_used_per_move: Duration::ZERO
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
            default: Some(options.minimum_time_used_per_move.as_millis() as i64),
            min: Some(0),
            max: Some(1000 * 60 * 60 * 24)
        } => |options, value| {
            let time = value
                .parse()
                .unwrap();
            options.minimum_time_used_per_move =
                Duration::from_millis(time);
        }
    }

    let (event_sink, events) = channel();
    std::thread::spawn({
        let event_sink = event_sink.clone();
        move || {
            let mut lines = BufReader::new(stdin()).lines();
            while let Some(Ok(line)) = lines.next() {
                let _ = event_sink.send(Event::UciMessage(vampirc_uci::parse_one(&line)));
            }
        }
    });

    'main: while let Ok(event) = events.recv() {
        match event {
            Event::UciMessage(message) => match message {
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
                    let time_manager;
                    time_manager = match time_control {
                        Some(UciTimeControl::MoveTime(time)) => StandardTimeManager::new(
                            Duration::ZERO,
                            0.0,
                            time.to_std().unwrap()
                        ),
                        Some(UciTimeControl::TimeLeft {
                            white_time,
                            black_time,
                            ..
                        }) => {
                            let (initial_pos, moves) = position.as_ref().unwrap();
                            let side_to_move = if moves.len() % 2 == 0 {
                                initial_pos.side_to_move()
                            } else {
                                !initial_pos.side_to_move()
                            };
                            let time_left = match side_to_move {
                                Color::White => white_time,
                                Color::Black => black_time
                            }.unwrap().to_std().unwrap();
                            StandardTimeManager::new(
                                time_left, 
                                options.percent_time_used_per_move,
                                options.minimum_time_used_per_move
                            )
                        }
                        Some(UciTimeControl::Ponder) => todo!(),
                        None | Some(UciTimeControl::Infinite) => StandardTimeManager::new(
                            Duration::ZERO,
                            0.0,
                            Duration::MAX
                        )
                    };
                    
                    options.search_options.max_depth = 64;
                    if let Some(search_control) = search_control {
                        if let Some(depth) = search_control.depth {
                            options.search_options.max_depth = depth;
                        }
                        //TODO implement the rest
                    }
                    let (initial_pos, moves) = position.take().unwrap();
                    let terminator = Arc::new(AtomicBool::new(false));
                    let mut handler = UciHandler {
                        time_manager,
                        search_begin: Instant::now(),
                        last_update: Instant::now(),
                        time_left: Duration::MAX,
                        search_terminator: Arc::clone(&terminator),
                        event_sink: event_sink.clone(),
                        prev_result: None,
                    };
                    std::thread::spawn({
                        let options = options.search_options.clone();
                        move || {
                            let mut search_state = LunaticSearchState::new(
                                &mut handler,
                                &initial_pos,
                                moves,
                                options
                            );
                            search_state.search();
                            handler.finish();
                        }
                    });
                    search = Some(terminator);
                }
                UciMessage::Stop => if let Some(search) = &mut search {
                    search.store(true, Ordering::Release);
                },
                
                UciMessage::PonderHit => {}
                UciMessage::Quit => break 'main,
                UciMessage::Register { .. } => {}
                UciMessage::Unknown(_, _) => {}
                //Engine to GUI messages
                _ => {}
            }
            Event::EngineSearchUpdate(result) => match result {
                EngineSearchResult::SearchInfo(result, duration) => {
                    let tt_filledness =
                        result.transposition_table_entries
                        * 1000
                        / result.transposition_table_size;
                    send_message(UciMessage::Info(vec![
                        match result.value.kind() {
                            EvaluationKind::Centipawn(cp) => UciInfoAttribute::from_centipawns(cp as i32),
                            EvaluationKind::MateIn(m) => UciInfoAttribute::from_mate(((m + 1) / 2) as i8),
                            EvaluationKind::MatedIn(m) => UciInfoAttribute::from_mate(-(((m + 1) / 2) as i8))
                        },
                        UciInfoAttribute::Depth(result.depth as u8),
                        UciInfoAttribute::Nodes(result.nodes as u64),
                        UciInfoAttribute::Pv(result.principal_variation.clone()),
                        UciInfoAttribute::Time(vampirc_uci::Duration::from_std(duration).unwrap()),
                        UciInfoAttribute::HashFull(tt_filledness as u16)
                    ]));
                }
                EngineSearchResult::SearchFinished(result) => {
                    send_message(UciMessage::best_move(result.mv));
                    search = None;
                }
            }
        }
    }
}
