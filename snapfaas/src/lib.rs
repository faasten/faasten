pub mod worker;
pub mod configs;
pub mod resource_manager;
pub mod syscalls;
// TODO what metrics do we want?
//pub mod metrics;
pub mod firecracker_wrapper;
pub mod blobstore;
pub mod labeled_fs;
pub mod fs;
pub mod sched;
pub mod vm;
pub mod syscall_server;

use std::string::String;
use std::io::{Read, Write, BufRead, BufReader};
use labeled::buckle;
use serde::Deserialize;
use sha2::Sha256;
use log::{error, warn};

//const LOCAL_FILE_URL_PREFIX: &str = "file://localhost";
const MEM_FILE: &str = "/proc/meminfo";     // meminfo file on linux
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


///// check if a string is a url string
///// TODO: maybe a more comprehensive check is needed but low priority
//pub fn check_url(path: &str) -> bool {
//    return path.starts_with("file://")
//         | path.starts_with("http://")
//         | path.starts_with("https://")
//         | path.starts_with("ftp://");
//}
//
//
///// Check if a path is local filesystem path. If yes, append file://localhost/;
///// If the path is already an URL, return itself.
///// This function supports absolute path and relative path containing only "." and "..".
///// An error is returned when the path does not exist.
//pub fn convert_fs_path_to_url (path: &str) -> Result<String> {
//    if check_url(path) {
//        return Ok(path.to_string());
//    }
//    let mut url = String::from(LOCAL_FILE_URL_PREFIX);
//    // unwrap is safe as the path is utf-8 valid
//    let path = std::fs::canonicalize(path).map(|p| p.to_str().unwrap().to_string())?;
//    url.push_str(path.as_str());
//
//    return Ok(url);
//}
//
///// Open a file specified by a URL in the form of a string
//pub fn open_url(url: &str) -> Result<std::fs::File> {
//    if !check_url(url) {
//        return Err(Error::new(ErrorKind::Other, "not Url"));
//    }
//
//    match Url::parse(url) {
//        Ok(url)=> {
//            //println!("{:?}", url);
//            //println!("{:?}", url.scheme());
//            //println!("{:?}", url.host());
//            //println!("{:?}", url.path());
//            //println!("{:?}", url.username());
//            return std::fs::File::open(url.path());
//        }
//        Err(_)=> return Err(Error::new(ErrorKind::Other, "Url parse failed")),
//    }
//
//}

pub fn get_machine_memory() -> usize {
    let memfile = std::fs::File::open(MEM_FILE).expect("Couldn't open /proc/meminfo");
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

/// The preparer installs supported kernels and runtime images in the directory ``home:^T,faasten''.
/// Kernels and runtime images are stored as blobs.
pub fn prepare_fs(config_path: &str) {
    #[derive(Deserialize)]
    struct Config {
        kernel: String,
        python: String,
        fsutil: String,
        other_runtimes: Vec<String>,
    }

    let config = std::fs::File::open(config_path)
        .expect("open configuration file");
    let config: Config = serde_yaml::from_reader(config).expect("deserialize");

    let faasten_fs = crate::fs::FS::new(&*crate::labeled_fs::DBENV);
    let mut blobstore = crate::blobstore::Blobstore::<Sha256>::default();
    let base_dir = &crate::syscall_server::str_to_syscall_path("home:^T,faasten").unwrap();
    let label = buckle::Buckle::parse("T,faasten").unwrap();

    // bootstrap
    if !faasten_fs.initialize() {
        warn!("Existing root detected. Noop. Exiting.");
    } else {
        // set up ``home''
        let rootpriv = buckle::Component::dc_false();
        crate::fs::utils::set_my_privilge(rootpriv);
        crate::fs::utils::create_faceted(&faasten_fs, &Vec::new(), "home".to_string())
            .expect("create ``home'' faceted directory");

        let faasten_principal = vec!["faasten".to_string()];
        crate::fs::utils::set_my_privilge([buckle::Clause::new_from_vec(vec![faasten_principal])].into());
        let kernel_blob = {
            let mut kernel = std::fs::File::open(config.kernel).expect("open kernel file");
            let mut blob = blobstore.create().expect("create kernel blob");
            let buf = &mut Vec::new();
            let _ = kernel.read_to_end(buf).expect("read kernel file");
            blob.write_all(buf).expect("write kernel blob");
            let blob = blobstore.save(blob).expect("finalize kernel blob");
            let name = "kernel".to_string();
            crate::fs::utils::create_blob(&faasten_fs, base_dir, name, label.clone(), blob.name.clone())
                .expect("link kernel blob");
            blob
        };

        let python_blob = {
            let mut python = std::fs::File::open(config.python).expect("open python file");
            let mut blob = blobstore.create().expect("create python blob");
            let buf = &mut Vec::new();
            let _ = python.read_to_end(buf).expect("read python file");
            blob.write_all(buf).expect("write python blob");
            let blob = blobstore.save(blob).expect("finalize python blob");
            let name = "python".to_string();
            crate::fs::utils::create_blob(&faasten_fs, base_dir, name, label.clone(), blob.name.clone())
                .expect("link python blob");
            blob
        };

        {
            let mut fsutil = std::fs::File::open(config.fsutil).expect("open fsutil file");
            let mut blob = blobstore.create().expect("create fsutil blob");
            let buf = &mut Vec::new();
            let _ = fsutil.read_to_end(buf).expect("read fsutil file");
            blob.write_all(buf).expect("write fsutil blob");
            let blob = blobstore.save(blob).expect("finalize fsutil blob");
            let f = crate::fs::Function {
                memory: 128,
                app_image: blob.name,
                runtime_image: python_blob.name,
                kernel: kernel_blob.name,
            };
            crate::fs::utils::create_gate(&faasten_fs, base_dir, "fsutil".to_string(), label.clone(), f)
                .expect("link fsutil blob");
        }

        for rt in config.other_runtimes {
            let mut img = std::fs::File::open(&rt).expect(&format!("open runtime image {:?}", rt));
            let mut blob = blobstore.create().expect(&format!("create runtime blob {:?}", rt));
            let buf = &mut Vec::new();
            let _ = img.read_to_end(buf).expect(&format!("read runtime file {:?}", rt));
            blob.write_all(buf).expect(&format!("write runtime blob {:?}", rt));
            let blob = blobstore.save(blob).expect(&format!("finalize runtime blob {:?}", rt));
            let name = std::path::Path::new(&rt).file_name().unwrap().to_str().unwrap().to_string();
            crate::fs::utils::create_blob(&faasten_fs, base_dir, name, label.clone(), blob.name)
                .expect(&format!("link {:?} blob", rt));
        }
    }
}
