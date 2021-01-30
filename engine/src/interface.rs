use serde::{Serialize, Deserialize};
use chess::*;
use std::sync::mpsc;
use std::future::Future;
use futures::channel::oneshot;

use crate::evaluation::*;
use crate::engine::*;

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
        board: Board,
        max_depth: u8
    },
    EndThink(oneshot::Sender<Option<(ChessMove, MoveInfo)>>)
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct MoveInfo {
    pub value: i32,
    pub nodes: u32,
    pub depth: u32
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
                if let LunaticContextCommand::BeginThink { board, max_depth } = command {
                    let mut search = LunaticSearchState::new();
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
                            mv = search.best_move(
                                &settings.evaluator,
                                &board,
                                depth as u8
                            );
                            depth += 1;
                            match thinker_recv.try_recv() {
                                Ok(command) => command,
                                Err(mpsc::TryRecvError::Empty) => continue,
                                Err(mpsc::TryRecvError::Disconnected) => return
                            }
                        };
                        if let LunaticContextCommand::EndThink(resolver) = command {
                            let mv = mv
                                .map(|(mv, info)| (mv, MoveInfo {
                                    value: info.value,
                                    nodes: info.nodes,
                                    depth
                                }));
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

    pub fn begin_think(&self, board: Board, max_depth: u8) {
        self.thinker.send(LunaticContextCommand::BeginThink {
            board,
            max_depth
        }).unwrap();
    }
    
    pub fn end_think(&self) -> impl Future<Output=Result<Option<(ChessMove, MoveInfo)>, oneshot::Canceled>>  {
        let (resolver, receiver) = oneshot::channel();
        self.thinker.send(LunaticContextCommand::EndThink(resolver)).unwrap();
        receiver
    }
}
