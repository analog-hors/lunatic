use serde::{Serialize, Deserialize};
use chess::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};

use std::time::{Duration, Instant};

use crate::evaluation::*;
use crate::engine::*;
pub use crate::engine::SearchResult;

pub struct SearchRequest(Arc<AtomicBool>, Receiver<Option<(SearchResult, Duration)>>);

impl SearchRequest {
    ///End the search. The search channel will no longer produce new outputs.
    pub fn terminate(&mut self) -> Option<(SearchResult, Duration)> {
        self.0.store(true, Ordering::Release);
        self.1.recv().unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LunaticContextSettings<E> {
    pub evaluator: E
}

impl<E: Default> Default for LunaticContextSettings<E> {
    fn default() -> Self {
        Self {
            evaluator: E::default()
        }
    }
}

struct SearchParams {
    initial_pos: Board,
    moves: Vec<ChessMove>,
    transposition_table_size: usize,
    max_depth: u8,
    options: SearchOptions,
    terminator: Arc<AtomicBool>,
    resolver: SyncSender<Option<(SearchResult, Duration)>>,
    info_channel: Sender<(SearchResult, Duration)>
}

#[derive(Debug)]
pub struct LunaticContext {
    thinker: Sender<SearchParams>
}

impl LunaticContext {
    pub fn new(settings: LunaticContextSettings<impl Evaluator + Send + 'static>) -> Self {
        let (thinker, thinker_recv) = channel();
        std::thread::spawn(move || {
            while let Ok(SearchParams {
                initial_pos,
                moves,
                transposition_table_size,
                max_depth,
                options,
                terminator,
                info_channel,
                resolver
            }) = thinker_recv.recv() {
                let search_start_time = Instant::now();
                let mut game_history = Vec::with_capacity(100);
                let mut board = initial_pos;
                game_history.push(board.get_hash());
                for mv in moves {
                    if crate::engine::move_resets_fifty_move_rule(mv, &board) {
                        game_history.clear();
                    }
                    board = board.make_move_new(mv);
                    game_history.push(board.get_hash());
                }
                let halfmove_clock = game_history.len() as u8;
                
                let mut search = LunaticSearchState::new(
                    transposition_table_size,
                    max_depth as usize
                );
                let mut search_result = None;
                
                let mut depth = 0;
                loop {
                    let mut finished = depth as u8 > max_depth;
                    if !finished {
                        let search = search.best_move(
                            &settings.evaluator,
                            &board,
                            &mut game_history,
                            halfmove_clock,
                            depth as u8,
                            &options,
                            &terminator
                        );
                        depth += 1;
                        match search {
                            Ok(result) => {
                                let result = (result, search_start_time.elapsed());
                                let _ = info_channel.send(result.clone());
                                search_result = Some(result);
                            }
                            Err(SearchError::Terminated) => finished = true,
                            Err(SearchError::NoMoves) => {}
                        }
                    }
                    if finished {
                        resolver.send(search_result).unwrap();
                        break;
                    }
                }
            }
        });
        LunaticContext {
            thinker
        }
    }

    pub fn begin_think(
        &self,
        initial_pos: Board,
        moves: Vec<ChessMove>,
        transposition_table_size: usize,
        max_depth: u8,
        options: SearchOptions
    ) -> (Receiver<(SearchResult, Duration)>, SearchRequest) {
        let (info_channel, info_channel_recv) = channel();
        let (resolver, result) = sync_channel(0);
        let terminator = Arc::new(AtomicBool::new(false));
        self.thinker.send(SearchParams {
            initial_pos,
            moves,
            transposition_table_size,
            max_depth,
            options,
            terminator: Arc::clone(&terminator),
            resolver,
            info_channel
        }).unwrap();
        (info_channel_recv, SearchRequest(terminator, result))
    }
}
