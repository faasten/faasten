use std::io::prelude::*;
use std::io::{BufReader, ErrorKind};
use std::net::TcpStream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use clap::{App, Arg};

use snapfaas::request;

fn main() {
    println!("client");
    let matches = App::new("SnapFaas Client")
        .version("1.0")
        .author("David H. Liu <hao.liu@princeton.edu>")
        .about("Client program for SnapFaaS")
        .arg(
            Arg::with_name("function")
                .short("f")
                .long("function")
                .takes_value(true)
                .help("name of the function to invoke")
        )
        .arg(
            Arg::with_name("data")
                .short("d")
                .long("data")
                .takes_value(true)
                .help("input data to target function")
        )
        .arg(
            Arg::with_name("input_file")
                .short("i")
                .long("input_file")
                .takes_value(true)
                .help("requests file that client will read from")
        )
        .arg(
            Arg::with_name("server addr")
                .short("s")
                .long("server")
                .takes_value(true)
                .help("server IP:port")
        )
        .get_matches();

    let mut stream = TcpStream::connect(matches.value_of("server addr").expect("server address not specified")).expect("failed to connect");
    //stream.set_nonblocking(true).expect("cannot set stream to non-blocking");

    let mut sc = stream.try_clone().expect("Cannot clone TcpStream");
    let receiver_thread = std::thread::spawn(move || {
        loop {

            match request::read_u8(&mut sc) {
                Ok(rsp) => {
                    println!("{:?}", String::from_utf8(rsp).expect("not json string"))
                }
                Err(e) => {
                    match e.kind() {
                        Other => { continue
                        }
                        _ => {
                            println!("Failed to read response: {:?}", e);
                        }
                    }
                }
            }
        }
    });

    if let Some(p)  = matches.value_of("input_file") {
        let mut reader = std::fs::File::open(p).map(|f|
            BufReader::new(f)).expect("Failed to open file");

        let mut prev_time = 0;
        loop {
            // read line as String
            let mut buf = String::new();
            if let Ok(s) = reader.read_line(&mut buf) {
                if s > 0 {
                    let req = request::parse_json(&buf).expect(&format!("cannot parse string: {}",buf));
                    std::thread::sleep(std::time::Duration::from_millis(req.time-prev_time));
                    prev_time = req.time;
                    println!("sending request: {:?}", buf);
                    if let Err(e) = request::write_u8(buf.as_bytes(), &mut stream) {
                        println!("Failed to send request: {:?}", e);
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    } else {

    }

    receiver_thread.join();
}
