use std::collections::VecDeque;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::io::ErrorKind;

use snapfaas::request;

/// one thread that accepts incoming connections. A pool of (one or more)
/// threads read from each accepted streams.
fn main() {
    let listener = TcpListener::bind("localhost:28888").expect("failed to bind");

    let streams: Arc<Mutex<VecDeque<TcpStream>>> = Arc::new(Mutex::new(VecDeque::new()));
    let mut handles:Vec<std::thread::JoinHandle<()>> = Vec::new();

    let num_reader = 1;

    // create reader threads (currently just one reader thread).
    // Reader threads iterator over streams in a round-robin order and read
    // the next request.
    for _ in 0..num_reader {
        let streams = streams.clone();
        let h = std::thread::spawn(move || {

            loop {

                // For each TcpStream in a shared VecDeque of TcpStream values,
                // try to read a request from it.
                // If there's no data in the stream, move on to the next one.
                // If the stream returns EOF, close the stream and remove it
                // from the VecDeque.
                let s = streams.lock().expect("stream lock poisoned").pop_front();
                match s {
                    None => continue,
                    Some(mut s) => {
                        match request::read_u8(&mut s) {
                            Ok(rsp) => {
                                // for this prototype, just echo the request back
                                match request::write_u8(&rsp, &mut s) {
                                    Ok(_) => (),
                                    Err(e) => println!("Echo failed: {:?}", e)
                                }
                                println!("request received: {:?}", String::from_utf8(rsp).expect("not json string"));
                            }
                            Err(e) => {
                                match e.kind() {
                                    // when client closed the connection, remove the
                                    // stream from stream list
                                    ErrorKind::UnexpectedEof => {
                                        println!("connection {:?} closed by client", s);
                                        drop(s);
                                        continue
                                    }
                                    ErrorKind::WouldBlock => {
                                    }
                                    _ => {
                                        // Some other error happened. Report and
                                        // just try the next stream in the list
                                        eprintln!("Failed to read response: {:?}", e);
                                    }
                                }
                            }
                        }

                        streams.lock().expect("stream lock poisoned").push_back(s);
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
        if let Ok(stream) = stream {
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

            stream.set_nonblocking(true).expect("cannot set stream to non-blocking");
            //stream.set_read_timeout(Some(std::time::Duration::new(0, 1000000)));
            {
                let mut streams = streams.lock().expect("can't lock stream list");
                streams.push_back(stream);
                println!("number of streams: {:?}", streams.len());
            }

            // `stream.read_to_end()` solves this problem.
            //let mut buf:Vec<u8> = Vec::with_capacity(1);
            //println!("{:?}", stream.read_to_end(&mut buf));
            //println!("{:?}", buf);
            //println!("{:?}", String::from_utf8(buf).expect("not json string"));
        }
    }

    for h in handles {
        h.join().expect("Couldn't join on the thread");
    }
}
