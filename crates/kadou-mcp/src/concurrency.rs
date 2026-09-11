//! Per-server concurrency limit and per-kata-id single-flight (`docs/design/05-prd.md` §6.1
//! "A per-server concurrency limit (default 2) caps parallel runs; one run at a time per
//! kata id, or the call returns `busy`.").

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct Concurrency {
    semaphore: Arc<Semaphore>,
    running_ids: Arc<Mutex<HashSet<String>>>,
}

/// Held for the lifetime of one `run_kata` execution; dropping it (including on an early
/// return or a panic) frees both the id lock and the concurrency permit.
pub struct RunGuard {
    _permit: OwnedSemaphorePermit,
    id: String,
    running_ids: Arc<Mutex<HashSet<String>>>,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.running_ids.lock().unwrap().remove(&self.id);
    }
}

impl Concurrency {
    pub fn new(limit: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(limit.max(1))),
            running_ids: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Reserves a slot for `id`. `Err(())` ("busy", §5.5) when `id` is already running or the
    /// server is at its concurrency limit.
    pub fn try_start(&self, id: &str) -> Result<RunGuard, ()> {
        {
            let mut set = self.running_ids.lock().expect("running_ids mutex poisoned");
            if set.contains(id) {
                return Err(());
            }
            set.insert(id.to_string());
        }
        match Arc::clone(&self.semaphore).try_acquire_owned() {
            Ok(permit) => Ok(RunGuard {
                _permit: permit,
                id: id.to_string(),
                running_ids: Arc::clone(&self.running_ids),
            }),
            Err(_) => {
                self.running_ids
                    .lock()
                    .expect("running_ids mutex poisoned")
                    .remove(id);
                Err(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_run_of_the_same_id_is_busy() {
        let c = Concurrency::new(2);
        let _first = c.try_start("sesami/cc4-aaa").unwrap();
        assert!(c.try_start("sesami/cc4-aaa").is_err());
    }

    #[test]
    fn dropping_a_guard_frees_the_id() {
        let c = Concurrency::new(2);
        {
            let _first = c.try_start("sesami/cc4-aaa").unwrap();
        }
        assert!(c.try_start("sesami/cc4-aaa").is_ok());
    }

    #[test]
    fn the_server_wide_limit_returns_busy_once_exhausted() {
        let c = Concurrency::new(1);
        let _a = c.try_start("a").unwrap();
        assert!(c.try_start("b").is_err());
    }
}
