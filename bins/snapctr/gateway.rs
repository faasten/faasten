
use std::io::BufReader;
use std::fs::File;

pub trait Gateway<T> {
    fn listen(source: &str) -> std::io::Result<T>;
//    fn incoming<T>(&self) -> T
//        where T: Iterator;
}

pub struct FileGateway {
    url: String,
//    file: File,
    reader: BufReader<File>,
}

impl FileGateway {
//    pub fn new() -> FileGateway {
//    }
}

impl Gateway<FileGateway> for FileGateway {
    fn listen(source: &str) -> std::io::Result<FileGateway>{
        match File::open(source) {
            Err(e) => Err(e),
            Ok(file) => {
                let reader = BufReader::new(file);
                Ok(FileGateway {
                    url: source.to_string(),
                    reader: reader,
                })
            }
        }
    }
}

