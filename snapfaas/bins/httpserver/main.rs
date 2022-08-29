use std::fs;
use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;

#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, SubCommand, Arg};

const IP:           &str = "127.0.0.1";
const COLON:        &str = ":";
const DEFAULT_PORT: &str = "8080";

const BUFSIZE: usize = 1024;

const GET:    &str = "GET";    // fs read()
const DELETE: &str = "DELETE"; // fs delete()
const POST:   &str = "POST";   // fs write()
const PUT:    &str = "PUT";    // fs create() (b/c idempotent)

const STATUS_OK:          &str = "HTTP/1.1 200 Ok\r\n\r\n";
#[allow(dead_code)] // FIXME remove when implement function(s) to access fs
const STATUS_CREATED:     &str = "HTTP/1.1 201 Created\r\n\r\n";
#[allow(dead_code)] // FIXME remove when implement function(s) to access fs
const STATUS_BAD_REQUEST: &str = "HTTP/1.1 400 Bad Request\r\n\r\n";
#[allow(dead_code)] // FIXME remove when implement function(s) to access fs
const STATUS_NOT_FOUND:   &str = "HTTP/1.1 404 Not Found\r\n\r\n";
const STATUS_NOT_ALLOWED: &str = "HTTP/1.1 505 Method Not Allowed\r\n\r\n";

fn main() {
  let cmd_arguments = App::new("httpserver")
      .version(crate_version!())
      .author(crate_authors!())
      .subcommand(
        SubCommand::with_name("port")
          .about("Specify port on which to listen for incoming connections")
          .arg(Arg::with_name("PORTNUM"))
      )
      .get_matches();

  let port = match cmd_arguments.subcommand() {
    ("port", Some(sub_m)) => {
      sub_m.value_of("PORTNUM").unwrap()
    },
    (&_, _) => {
      DEFAULT_PORT
    }
  };

  let addr: String = vec![IP, COLON, port].concat();
  let listener = TcpListener::bind(addr).unwrap();

  for stream in listener.incoming() {
    match stream {
      Ok(stream) => handle_connection(stream),
      Err(e) => println!("Error: {:?}", e),
    }
  }
}

// mock version of FS interaction is sufficient for pull req
fn handle_connection(mut stream: TcpStream) {
  // TODO handle streams larger than BUFSIZE bytes
  let mut buffer = [0; BUFSIZE];
  match stream.read(&mut buffer) {
    Ok(_bytes_read) => {
      let mut headers = [httparse::EMPTY_HEADER; 8];
      let mut req = httparse::Request::new(&mut headers);
      match req.parse(&buffer) {
        Ok(status) => {
          match status {
            httparse::Status::Complete(body_offset) => {
              let body = &buffer[body_offset..];
              let body_str = String::from_utf8_lossy(&body[..]);
              let body_str_trimmed = body_str.trim_end();
              let (status_code, contents_out_op) = match req.method {
                Some(method) => {
                  let (status_code, contents_out_op) = match method.to_uppercase().as_str() {
                    GET => {
                      // TODO access labeled fs
                      println!("GET req for path: {:?}", req.path);
                      (STATUS_OK, Some(fs::read_to_string("dummy_out.txt").unwrap()))
                    },
                    DELETE => {
                      // TODO access labeled fs
                      println!("DELETE req for path: {:?}", req.path);
                      (STATUS_OK, None)
                    },
                    POST => {
                      // TODO access labeled fs
                      println!("POST req for path: {:?}", req.path);
                      println!("  data: {}", body_str_trimmed);
                      (STATUS_OK, None)
                    },
                    PUT => {
                      // TODO access labeled fs
                      println!("PUT req for path: {:?}", req.path);
                      println!("  data: {}", body_str_trimmed);
                      (STATUS_OK, None)
                    },
                    _ => (STATUS_NOT_ALLOWED, None),
                  };
                  (status_code, contents_out_op)
                },
                None => (STATUS_BAD_REQUEST, None),
              };

              // TODO build more robust response
              let response = if contents_out_op.is_some() {
                let contents_out = contents_out_op.unwrap();
                format!(
                  "{}\r\nContent-Length: {}\r\n\r\n{}",
                  status_code,
                  contents_out.len(),
                  contents_out
                )
              } else {
                format!(
                  "{}\r\nContent-Length: {}\r\n\r\n",
                  status_code,
                  0
                )
              };

              // write response
              match stream.write(response.as_bytes()) {
                Ok(_) => {
                  match stream.flush() {
                    Ok(_) => println!("Success!"),
                    Err(e) => println!("Error flushing response: {:?}", e),
                  }
                },
                Err(e) => println!("Error writing response: {:?}", e),
              };
            },
            httparse::Status::Partial => println!("Parse only partially completed"),
          };
        },
        Err(e) => println!("Error parsing request: {}", e),
      };
    },
    Err(e) => println!("Error reading request: {:?}", e),
  }
}
