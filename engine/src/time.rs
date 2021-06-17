use std::time::Duration;

use crate::engine::SearchResult;
use crate::evaluator::*;

pub trait TimeManager {
    ///Update the time manager's internal state with a new result.
    ///`time` represents the duration since the last update.
    ///Returns a timeout to the next update; If no update happens before
    ///the timeout, stop searching.
    fn update(&mut self, result: SearchResult, time: Duration) -> Duration;
}

///Extremely naive time manager that only uses a fixed amount of time per move.
pub struct FixedTimeManager {
    interval: Duration,
    elapsed: Duration
}

impl FixedTimeManager {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            elapsed: Duration::from_secs(0)
        }
    }
}

impl TimeManager for FixedTimeManager {
    fn update(&mut self, _: SearchResult, time: Duration) -> Duration {
        self.elapsed += time;
        if self.interval > self.elapsed {
            self.interval - self.elapsed
        } else {
            Duration::from_secs(0)
        }
    }
}

///Extremely naive time manager that only uses a fixed percentage of time per move
pub struct PercentageTimeManager(FixedTimeManager);

impl PercentageTimeManager {
    pub fn new(time_left: Duration, percentage: f32, minimum_time: Duration) -> Self {
        Self(FixedTimeManager::new(time_left.mul_f32(percentage).max(minimum_time)))
    }
}

impl TimeManager for PercentageTimeManager {
    fn update(&mut self, result: SearchResult, time: Duration) -> Duration {
        self.0.update(result, time)
    }
}

///The standard time manager. Still quite naive.
pub struct StandardTimeManager(PercentageTimeManager);

impl StandardTimeManager {
    pub fn new(time_left: Duration, percentage: f32, minimum_time: Duration) -> Self {
        Self(PercentageTimeManager::new(time_left, percentage, minimum_time))
    }
}

impl TimeManager for StandardTimeManager {
    fn update(&mut self, result: SearchResult, time: Duration) -> Duration {
        if let EvaluationKind::Centipawn(_) = result.value.kind() {
            self.0.update(result, time)
        } else {
            //Forced outcome, cut thinking short
            Duration::from_secs(0)
        }
    }
}
