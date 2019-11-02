
use std::io::{BufReader, Lines};
use std::fs::File;
use std::io::BufRead;

pub trait Gateway<T, I> {
    fn listen(source: &str) -> std::io::Result<T>;
    fn incoming(self) -> I
        where I: Iterator;
}

#[derive(Debug)]
pub struct FileGateway {
    url: String,
    reader: BufReader<File>,
}

impl Gateway<FileGateway, Lines<BufReader<File>>> for FileGateway {
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

    fn incoming(self) -> Lines<BufReader<File>> {
        return self.reader.lines();
    }
}

