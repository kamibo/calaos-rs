use std::cmp::Ordering;
use std::time::Duration;
use std::time::Instant;

use crate::io_value;

use io_value::IOValue;

#[derive(Eq, PartialEq)]
pub struct Task {
    pub deadline: Instant,
    pub target_state: IOValue,
}

impl Task {
    pub fn from_now(duration: Duration, target_state: IOValue) -> Self {
        Self {
            deadline: Instant::now() + duration,
            target_state,
        }
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deadline.cmp(&other.deadline)
    }
}
