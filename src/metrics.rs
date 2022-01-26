use std::{thread, time};
use std::sync::{Arc, Mutex};
use std::io::Write;
use std::fs::File;

use log::error;

// one hour
const FLUSH_INTERVAL_SECS: u64 = 3600;

#[derive(Default, Debug)]
pub struct RequestTimestamps {
    /// request arrival time
    pub arrived: u64,
    /// resource allocation completion time, 0 if resource exhaution
    pub allocated: u64,
    /// VM launch completion time, 0 if launching fails
    pub launched: u64,
    /// response returned time, 0 if execution fails
    pub completed: u64,
}

#[derive(Debug)]
pub struct WorkerMetrics {
    log_file: File,
    request_timestamps: Arc<Mutex<Vec<RequestTimestamps>>>,
}

impl WorkerMetrics {
    pub fn new(log_file: File) -> Self {
        WorkerMetrics {
            log_file,
            request_timestamps: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start_timed_flush(&self) {
        let reqtsps = Arc::clone(&self.request_timestamps);
        let mut log_file = self.log_file.try_clone().unwrap();
        thread::spawn(move || {
            loop {
                thread::sleep(time::Duration::from_secs(FLUSH_INTERVAL_SECS));
                let tsps = &mut *reqtsps.lock().unwrap();
                for t in tsps.as_slice() {
                    if let Err(e) = log_file.write_fmt(format_args!("{:?}", t)) {
                        error!("failed to flush worker metrics: {:?}", e);
                    }
                }
                tsps.truncate(0);
            }
        });
    }

    /// insert a request's timestamps
    pub fn push(&mut self, tsps: RequestTimestamps) {
        self.request_timestamps.lock().unwrap().push(tsps);
    }

    /// manual flush
    pub fn flush(mut self) {
        let tsps = &*self.request_timestamps.lock().unwrap();
        for t in tsps.as_slice() {
            if let Err(e) = self.log_file.write_fmt(format_args!("{:?}", t)) {
                error!("failed to flush worker metrics: {:?}", e);
            }
        }
    }
}
