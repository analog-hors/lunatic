use serde::{Serialize, Deserialize};
use chess::*;
use std::sync::mpsc;
use futures::channel::oneshot;

use crate::evaluation::*;
use crate::engine::*;
pub use crate::engine::SearchInfo;

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

enum LunaticContextCommand {
    BeginThink {
        initial_pos: Board,
        moves: Vec<ChessMove>,
        transposition_table_size: usize,
        max_depth: u8,
        info_channel: mpsc::Sender<SearchInfo>
    },
    EndThink(oneshot::Sender<Option<ChessMove>>)
}

#[derive(Debug)]
pub struct LunaticContext {
    thinker: mpsc::Sender<LunaticContextCommand>
}

impl LunaticContext {
    pub fn new(settings: LunaticContextSettings<impl Evaluator + Send + 'static>) -> Self {
        let (thinker, thinker_recv) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(command) = thinker_recv.recv() {
                if let LunaticContextCommand::BeginThink {
                    initial_pos,
                    moves,
                    transposition_table_size,
                    max_depth,
                    info_channel
                } = command {
                    let mut game_history = Vec::with_capacity(100);
                    let mut board = initial_pos;
                    game_history.push(board.get_hash());
                    for mv in moves {
                        if crate::engine::move_zeroes(mv, &board) {
                            game_history.clear();
                        }
                        board = board.make_move_new(mv);
                        game_history.push(board.get_hash());
                    }
                    let depth_since_zeroing = game_history.len() as u8;
                    
                    let mut search = LunaticSearchState::new(
                        transposition_table_size,
                        max_depth as usize
                    );
                    let mut mv = None;
                    let mut depth = 0;
                    loop {
                        let command = if depth as u8 > max_depth {
                            if let Ok(recv) = thinker_recv.recv() {
                                recv
                            } else {
                                return
                            }
                        } else {
                            let search = search.best_move(
                                &settings.evaluator,
                                &board,
                                &mut game_history,
                                depth_since_zeroing,
                                depth as u8
                            );
                            if let Some((m, info)) = search {
                                mv = Some(m);
                                let _ = info_channel.send(info);
                            }
                            depth += 1;
                            match thinker_recv.try_recv() {
                                Ok(command) => command,
                                Err(mpsc::TryRecvError::Empty) => continue,
                                Err(mpsc::TryRecvError::Disconnected) => return
                            }
                        };
                        if let LunaticContextCommand::EndThink(resolver) = command {
                            let _ = resolver.send(mv);
                            break;
                        }
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
        max_depth: u8
    ) -> mpsc::Receiver<SearchInfo> {
        let (info_channel, info_channel_recv) = mpsc::channel();
        self.thinker.send(LunaticContextCommand::BeginThink {
            initial_pos,
            moves,
            transposition_table_size,
            max_depth,
            info_channel
        }).unwrap();
        info_channel_recv
    }
    
    pub async fn end_think(&self) -> Result<Option<ChessMove>, oneshot::Canceled>  {
        let (resolver, receiver) = oneshot::channel();
        self.thinker.send(LunaticContextCommand::EndThink(resolver)).unwrap();
        receiver.await
    }
}
