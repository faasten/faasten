use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// one thread that accepts incoming connections. A pool of (one or more)
/// threads read from each accepted streams.
fn main() {
    println!("Hello, world!");
    let listener = TcpListener::bind("localhost:28888").expect("failed to bind");

    let streams: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles:Vec<std::thread::JoinHandle<()>> = Vec::new();

    let num_reader = 1;

    for _ in 0..num_reader {
        let streams = streams.clone();
        // create reader threads (currently just one reader thread).
        // Reader threads iterator over streams in a round-robin order and read
        // the next request.
        let h = std::thread::spawn(move || {
            let mut pt = 0;

            loop {
                let mut s = streams.lock().expect("can't acquire read lock");
                if s.len() >0 {
                    let mut buf = [0;4];
                    if let Err(e) = s[pt].read_exact(&mut buf) {
                            match e.kind() {
                                std::io::ErrorKind::UnexpectedEof => {
                                    s.remove(pt);
                                }
                                _ => println!("Failed to read request length")
                            }
                            continue
                    }

                    let size = u32::from_be_bytes(buf);
                    if size <= 0 {
                        pt = pt+1;
                        if pt >= s.len() {
                            pt = 0;
                        }
                        continue
                    }

                    println!("size: {}",size);

                    let mut buf = vec![0; size as usize];

                    match s[pt].read_exact(&mut buf) {
                        Ok(size) => {
                            s[pt].write_all(&buf.len().to_be_bytes());
                            s[pt].write_all(&buf);
                            println!("{:?}", String::from_utf8(buf.to_vec()).expect("not json string"));
                        }
                        Err(e) => {
                            match e.kind() {
                                // when client closed the connection, remove the
                                // stream from stream list
                                std::io::ErrorKind::UnexpectedEof => {
                                    println!("unexpected EOF");
                                    // do not increment pt because s.remove()
                                    // will move all elements in the array to
                                    // the left once.
                                    s.remove(pt);
                                    continue
                                }
                                _ => println!("Failed to read request")
                            }
                        }

                    }

                    pt = pt+1;
                    if pt >= s.len() {
                        pt = 0;
                    }
                }
            }
        });
        handles.push(h);
    }

    for stream in listener.incoming() {
        println!("New connection:");
        println!("{:?}", stream);
        // ignore Err streams
        if let Ok(mut stream) = stream {
            // read requests from the stream
            // TODO: what if the request is larger than allocated buffer?
            // Answer: at least when we use `stream.read()`, it will just read
            // up to the size of the buffer and anything left unread stays in
            // the stream.
            // TODO: what if the request is smaller than allocated buffer? How
            // to decide the EOF of the request?
            // Answer: at least when we use `stream.read()`, part of the buffer
            // not modified by `read()` will just keep its previous values.
            // TODO: should I use a zeroed-out buffer every time? if so, should
            // I just allocate a new buffer every time or zero out an existing
            // buffer?
            // Answer: the goal is to know the length of the data read in. It
            // doesn't matter whether the buffer is zeroed-out or not; `read()`
            // will just overwrite existing values.
            /*
            let mut buf = [0;2];
            println!("{:?}", stream.read_exact(&mut buf));
            println!("{:?}", buf);
            println!("{:?}", stream.read_exact(&mut buf));
            println!("{:?}", buf);
            */

            {
                let mut streams = streams.lock().expect("can't lock stream list");
                streams.push(stream);
                println!("number of streams: {:?}", streams.len());
            }

            // `stream.read_to_end()` solves this problem.
            //let mut buf:Vec<u8> = Vec::with_capacity(1);
            //println!("{:?}", stream.read_to_end(&mut buf));
            //println!("{:?}", buf);
            //println!("{:?}", String::from_utf8(buf).expect("not json string"));
        }
    }
}
