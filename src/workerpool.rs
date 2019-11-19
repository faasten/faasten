//! A fixed size pool (maybe slightly below max, max being total memory/120MB)
//! Acquire a free worker from a pool. This should always succeed because we
//! should not run out of worker threads.
//! A worker takes a reqeust and finds a VM to execute it. 

use crate::worker::Worker;

const DEFAULT_NUM_WORKERS: u32 = 10;

pub struct WorkerPool {
    pool: Vec<Worker>,
    max_num_workers: u32,
    num_free: u32,
}

impl WorkerPool {
    pub fn new() -> WorkerPool {
        let mut pool = Vec::with_capacity(10);
        for _ in 0..10 {
            pool.push(Worker::new());
        }

        WorkerPool {
            pool: pool,
            max_num_workers: 10,
            num_free: 10,
        }
    }

    pub fn acquire(&mut self) -> Worker {
        return self.pool.pop().expect("Worker pool is empty");
    }
}
