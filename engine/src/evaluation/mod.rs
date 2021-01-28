mod standard;
pub use standard::*;

pub trait Evaluator {
    fn evaluate(&self, board: &chess::Board) -> i32;
}
