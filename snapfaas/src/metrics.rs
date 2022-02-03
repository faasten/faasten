use std::{thread, time};
use std::sync::{Arc, Mutex};
use std::io::Write;
use std::fs::File;

use log::error;
use serde_json;
use serde::Serialize;

use crate::request::Request;

#[derive(Default, Debug, Serialize)]
pub struct RequestTimestamps {
    /// time a request arrives at the gateway
    pub at_gateway: u64,
    /// time a request arrives at the VMM invoke handler
    pub at_vmm: u64,
    /// time a request arrives at a worker
    pub arrived: u64,
    /// resource allocation completion time, 0 if resource exhaution
    pub allocated: u64,
    /// VM launch completion time, 0 if launching fails
    pub launched: u64,
    /// response returned time, 0 if execution fails
    pub completed: u64,
    /// request in bytes
    pub request: Request,
}

impl RequestTimestamps {
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self).unwrap()
    }
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

    pub fn start_timed_flush(&self, interval: u64) {
        let reqtsps = Arc::clone(&self.request_timestamps);
        let mut log_file = self.log_file.try_clone().unwrap();
        thread::spawn(move || {
            loop {
                thread::sleep(time::Duration::from_secs(interval));
                let tsps = &mut *reqtsps.lock().unwrap();
                for t in tsps.as_slice() {
                    if let Err(e) = writeln!(&mut log_file, "{}", t.to_json()) {
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
            if let Err(e) = writeln!(&mut self.log_file, "{}", t.to_json()) {
                error!("failed to flush worker metrics: {:?}", e);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.request_timestamps.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    extern crate tempfile;
    use tempfile::NamedTempFile;

    use super::*;
    use std::io::{BufRead, BufReader};
    
    #[test]
    fn test_timed_flush() {
        let temp = NamedTempFile::new().unwrap();
        let mut m = WorkerMetrics::new(temp.reopen().unwrap());
        m.start_timed_flush(2);

        // test empty flush
        thread::sleep(time::Duration::from_secs(3));
        assert_eq!(m.len(), 0);

        // test non-empty flush
        m.push(Default::default());
        m.push(Default::default());
        assert_eq!(m.len(), 2);
        thread::sleep(time::Duration::from_secs(2));
        assert_eq!(m.len(), 0);

        m.push(Default::default());
        assert_eq!(m.len(), 1);
        thread::sleep(time::Duration::from_secs(2));
        assert_eq!(m.len(), 0);

        let breader = BufReader::new(temp.reopen().unwrap());
        let mut counter = 0;
        for l in breader.lines() {
            let tsps_json = RequestTimestamps { ..Default::default() }.to_json();
            assert_eq!(l.unwrap(), tsps_json);
            counter += 1;
        }
        assert_eq!(counter, 3);
    }

    #[test]
    fn test_manual_flush() {
        let temp = NamedTempFile::new().unwrap();
        let mut m = WorkerMetrics::new(temp.reopen().unwrap());
        m.start_timed_flush(2);
        m.push(Default::default());
        m.push(Default::default());
        assert_eq!(m.len(), 2);
        thread::sleep(time::Duration::from_secs(3));
        assert_eq!(m.len(), 0);

        m.push(Default::default());
        assert_eq!(m.len(), 1);
        m.flush();

        let breader = BufReader::new(temp.reopen().unwrap());
        let mut counter = 0;
        for l in breader.lines() {
            let tsps_json = RequestTimestamps { ..Default::default() }.to_json();
            assert_eq!(l.unwrap(), tsps_json);
            counter += 1;
        }
        assert_eq!(counter, 3);
    }
}
