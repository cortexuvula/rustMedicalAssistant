//! Background processing queue. Full implementation in Plan 2/3.

pub struct ProcessingQueueRepo;

impl ProcessingQueueRepo {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProcessingQueueRepo {
    fn default() -> Self {
        Self::new()
    }
}
