use std::io::prelude::*;
use std::io::{BufReader, ErrorKind};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use clap::{App, Arg};
use signal_hook::{iterator::Signals, SIGINT};
use crossbeam_channel::{bounded, Receiver, select};

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

    // create a response receiver thread that reads from the same TcpStream
    let num_rsp: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let num_req: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let mut sc = stream.try_clone().expect("Cannot clone TcpStream");
    let num_rspc = num_rsp.clone();
    let num_reqc = num_req.clone();
    let receiver_thread = std::thread::spawn(move || {
        loop {

            match request::read_u8(&mut sc) {
                Ok(rsp) => {
                    println!("{:?}", String::from_utf8(rsp).expect("not json string"));
                    let mut num_rsp = num_rspc.lock().expect("lock poisoned");
                    *num_rsp +=1;
                }
                Err(e) => {
                    match e.kind() {
                        Other => {
                            continue
                        }
                        _ => {
                            println!("Failed to read response: {:?}", e);
                        }
                    }
                }
            }
        }
    });

    // register signal handler                                            
    let num_rspc = num_rsp.clone();
    let signals = Signals::new(&[SIGINT]).expect("cannot create signals");
    std::thread::spawn(move || {                                          
	for sig in signals.forever() {                                    
	    println!("Received signal {:?}", sig);                        
            let num_req = num_reqc.lock().expect("num_req lock");
            let num_rsp = num_rspc.lock().expect("num_rsp lock");
	    println!("# Requests: {:?}", num_req);                        
	    println!("# Responses: {:?}", num_rsp);                        
            std::process::exit(0);
	}                                                                 
    });                                                                   


    if let Some(p)  = matches.value_of("input_file") {
        let mut reader = std::fs::File::open(p).map(|f|
            BufReader::new(f)).expect("Failed to open file");

        let mut prev_time = 0;
        loop {
            // read line as String
            let mut buf = String::new();
            match reader.read_line(&mut buf) {
                Ok(0) => {
                    // Ok(0) should indicate that we're at the end of the file
                    println!("read_line() returned 0 bytes");
                    break;
                }
                Ok(_n) => {
                    let req = request::parse_json(&buf).expect(&format!("cannot parse string: {}",buf));
                    std::thread::sleep(std::time::Duration::from_millis(req.time-prev_time));
                    prev_time = req.time;
                    println!("sending request: {:?}", buf);
                    if let Err(e) = request::write_u8(buf.as_bytes(), &mut stream) {
                        println!("Failed to send request: {:?}", e);
                    } else {
                        let mut num_req= num_req.lock().expect("lock poisoned");
                        *num_req+=1;
                    }
                }
                Err(e) => {
                    println!("read_line() returned error: {:?}", e);
                }
            }
        }
    } else {

    }

    loop {
        let cond = {
            let num_req = num_req.lock().expect("num_req lock");
            let num_rsp = num_rsp.lock().expect("num_rsp lock");
            *num_rsp < *num_req 
        };
        if cond {
            std::thread::sleep(std::time::Duration::from_millis(1000));
        } else {
	    println!("# Requests: {:?}", num_req);                        
	    println!("# Responses: {:?}", num_rsp);                        
            std::process::exit(0);
        }
    }
}
