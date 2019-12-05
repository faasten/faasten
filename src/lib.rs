pub mod request;
pub mod worker;
pub mod workerpool;
pub mod message;
pub mod gateway;
pub mod configs;
pub mod controller;
pub mod vm;

use std::string::String;
use std::fs::File;
use std::io::{BufReader, BufRead, Error, ErrorKind, Result};
use url::Url;

const LOCAL_FILE_URL_PREFIX: &str = "file://localhost";
const MEM_FILE: &str = "/proc/meminfo";     // meminfo file on linux
const KB_IN_MB: usize = 1024;
const MEM_4G: usize = 4096;  // in MB

/// check if a string is a url string
/// TODO: maybe a more comprehensive check is needed but low priority
pub fn check_url(path: &str) -> bool {
    return path.starts_with("file://")
         | path.starts_with("http://")
         | path.starts_with("https://")
         | path.starts_with("ftp://");
}


/// Check is a path is local filesystem path. If yes,
/// append file://localhost/ to local filesystem paths and expand ., .. and ~.
/// TODO: maybe a more comprehensive implementation is needed but low priority
pub fn convert_fs_path_to_url (path: &str) -> String {
    if check_url(path) {
        return path.to_string();
    }
    let mut url = String::from(LOCAL_FILE_URL_PREFIX);
    url.push_str(&shellexpand::tilde(path));

    return url;
}

/// Open a file specified by a URL in the form of a string
pub fn open_url(url: &str) -> Result<File> {
    if !check_url(url) {
        return Err(Error::new(ErrorKind::Other, "not Url"));
    }

    match Url::parse(url) {
        Ok(url)=> {
            //println!("{:?}", url);
            //println!("{:?}", url.scheme());
            //println!("{:?}", url.host());
            //println!("{:?}", url.path());
            //println!("{:?}", url.username());
            return File::open(url.path());
        }
        Err(_)=> return Err(Error::new(ErrorKind::Other, "Url parse failed")),
    }
    
}

pub fn get_machine_memory() -> usize {
    let memfile = File::open(MEM_FILE).expect("Couldn't open /proc/meminfo");
    for line in BufReader::new(memfile).lines() {
        match line {
            Ok(c) => {
                let parts: Vec<&str> = c.split(':').map(|s| s.trim()).collect();
                if parts[0] == "MemTotal" {
                    let mut mem = parts[1].split(' ').collect::<Vec<&str>>()[0]
                                  .parse::<usize>().unwrap();
                    mem = mem / KB_IN_MB;
                    return mem;
                }
            },
            Err(e) => {
                panic!("Reading meminfo file error: {:?}", e);
            }
        }
    }
    panic!("Cannot file MemTotal in /proc/meminfo");
}
