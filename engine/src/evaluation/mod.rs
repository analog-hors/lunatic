mod standard;
pub use standard::*;

pub trait Evaluator {
    fn evaluate(&self, board: &chess::Board, depth: u8) -> i32;
}
