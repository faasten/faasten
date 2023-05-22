pub mod configs;
pub mod resource_manager;
pub mod syscalls;
pub mod worker;
// TODO what metrics do we want?
//pub mod metrics;
pub mod blobstore;
pub mod cli;
pub mod firecracker_wrapper;
pub mod fs;
pub mod sched;
pub mod syscall_server;
pub mod vm;

use log::error;
use std::io::{BufRead, BufReader};

const MEM_FILE: &str = "/proc/meminfo"; // meminfo file on linux
const KB_IN_MB: usize = 1024;

/// rm worker*
pub fn unlink_unix_sockets() {
    match glob::glob("worker-*sock*") {
        Err(_) => error!("Invalid file pattern"),
        Ok(paths) => {
            for entry in paths {
                if let Ok(path) = entry {
                    if let Err(e) = std::fs::remove_file(&path) {
                        error!("Failed to unlink {}: {:?}", path.to_str().unwrap(), e);
                    }
                }
            }
        }
    }

    match glob::glob("dump_ws-*sock*") {
        Err(_) => error!("Invalid file pattern"),
        Ok(paths) => {
            for entry in paths {
                if let Ok(path) = entry {
                    if let Err(e) = std::fs::remove_file(&path) {
                        error!("Failed to unlink {}: {:?}", path.to_str().unwrap(), e);
                    }
                }
            }
        }
    }
}

pub fn get_machine_memory() -> usize {
    let memfile = std::fs::File::open(MEM_FILE).expect("Couldn't open /proc/meminfo");
    for line in BufReader::new(memfile).lines() {
        match line {
            Ok(c) => {
                let parts: Vec<&str> = c.split(':').map(|s| s.trim()).collect();
                if parts[0] == "MemTotal" {
                    let mut mem = parts[1].split(' ').collect::<Vec<&str>>()[0]
                        .parse::<usize>()
                        .unwrap();
                    mem = mem / KB_IN_MB;
                    return mem;
                }
            }
            Err(e) => {
                panic!("Reading meminfo file error: {:?}", e);
            }
        }
    }
    panic!("Cannot file MemTotal in /proc/meminfo");
}
