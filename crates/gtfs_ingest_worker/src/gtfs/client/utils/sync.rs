use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub struct SyncFailure {
    pub source_slug: String,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct SyncCommandOutcome<T> {
    pub succeeded: Vec<T>,
    pub failed: Vec<SyncFailure>,
}

impl<T> SyncCommandOutcome<T> {
    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }

    pub fn total_count(&self) -> usize {
        self.succeeded.len() + self.failed.len()
    }
}

pub fn partition_sync_results<T>(results: Vec<Result<T, SyncFailure>>) -> SyncCommandOutcome<T> {
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for result in results {
        match result {
            Ok(outcome) => succeeded.push(outcome),
            Err(failure) => failed.push(failure),
        }
    }

    SyncCommandOutcome { succeeded, failed }
}

pub struct SyncLogCounters {
    total_count: usize,
    succeeded_count: AtomicUsize,
    failed_count: AtomicUsize,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct SyncLogProgress {
    succeeded_count: usize,
    failed_count: usize,
    remaining_count: usize,
}

impl SyncLogCounters {
    pub fn new(total_count: usize) -> Self {
        Self {
            total_count,
            succeeded_count: AtomicUsize::new(0),
            failed_count: AtomicUsize::new(0),
        }
    }

    pub fn record_success(&self) -> SyncLogProgress {
        let succeeded_count = self.succeeded_count.fetch_add(1, Ordering::AcqRel) + 1;
        let failed_count = self.failed_count.load(Ordering::Acquire);
        self.progress(succeeded_count, failed_count)
    }

    pub fn record_failure(&self) -> SyncLogProgress {
        let failed_count = self.failed_count.fetch_add(1, Ordering::AcqRel) + 1;
        let succeeded_count = self.succeeded_count.load(Ordering::Acquire);
        self.progress(succeeded_count, failed_count)
    }

    fn progress(&self, succeeded_count: usize, failed_count: usize) -> SyncLogProgress {
        SyncLogProgress {
            succeeded_count,
            failed_count,
            remaining_count: self
                .total_count
                .saturating_sub(succeeded_count + failed_count),
        }
    }
}
