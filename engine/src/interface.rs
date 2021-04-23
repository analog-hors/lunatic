use serde::{Serialize, Deserialize};
use chess::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};

use std::time::{Duration, Instant};

use crate::evaluation::*;
use crate::engine::*;
pub use crate::engine::SearchResult;
use crate::oracle::Oracle;

pub struct SearchRequest(Arc<AtomicBool>, Receiver<Option<ContextSearchResult>>);

impl SearchRequest {
    ///End the search. The search channel will no longer produce new outputs.
    pub fn terminate(&mut self) -> Option<ContextSearchResult> {
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

#[derive(Debug, Clone)]
pub struct ContextSearchResult {
    pub result: SearchResult,
    pub search_duration: Duration,
    pub total_nodes_searched: u32,
    pub total_search_duration: Duration
}

struct SearchParams {
    initial_pos: Board,
    moves: Vec<ChessMove>,
    transposition_table_size: usize,
    max_depth: u8,
    options: SearchOptions,
    oracle: Arc<Oracle>,
    terminator: Arc<AtomicBool>,
    resolver: SyncSender<Option<ContextSearchResult>>,
    info_channel: Sender<ContextSearchResult>
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
                oracle,
                terminator,
                info_channel,
                resolver
            }) = thinker_recv.recv() {
                let search_start_time = Instant::now();
                let mut history = Vec::with_capacity(100);
                let mut board = initial_pos;
                history.push(board.get_hash());
                for mv in moves {
                    if crate::engine::move_resets_fifty_move_rule(mv, &board) {
                        history.clear();
                    }
                    board = board.make_move_new(mv);
                    history.push(board.get_hash());
                }
                
                let halfmove_clock = history.len() as u8;
                
                let mut search = LunaticSearchState::new(
                    &board,
                    &settings.evaluator,
                    &history,
                    halfmove_clock,
                    &options,
                    &*oracle,
                    transposition_table_size,
                    max_depth
                );
                let mut search_result = None;
                
                let mut nodes = 0;
                loop {
                    let iteration_start_time = Instant::now();
                    let search = search.deepen(&terminator);
                    match search {
                        Ok(result) => {
                            nodes += result.nodes;
                            let result = ContextSearchResult {
                                result,
                                search_duration: iteration_start_time.elapsed(),
                                total_nodes_searched: nodes,
                                total_search_duration: search_start_time.elapsed()
                            };
                            let _ = info_channel.send(result.clone());
                            search_result = Some(result);
                        }
                        Err(SearchError::Terminated) | Err(SearchError::MaxDepth) => break,
                        Err(SearchError::NoMoves) => {}
                    }
                }
                resolver.send(search_result).unwrap();
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
        options: SearchOptions,
        oracle: Arc<Oracle>
    ) -> (Receiver<ContextSearchResult>, SearchRequest) {
        let (info_channel, info_channel_recv) = channel();
        let (resolver, result) = sync_channel(0);
        let terminator = Arc::new(AtomicBool::new(false));
        self.thinker.send(SearchParams {
            initial_pos,
            moves,
            transposition_table_size,
            max_depth,
            options,
            oracle,
            terminator: Arc::clone(&terminator),
            resolver,
            info_channel
        }).unwrap();
        (info_channel_recv, SearchRequest(terminator, result))
    }
}
