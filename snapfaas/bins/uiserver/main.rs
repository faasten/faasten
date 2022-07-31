use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;

const BUFSIZE: usize = 1024;
const GET: &str = "GET"; // fs read()
const POST: &str = "POST"; // fs write()
const PUT: &str = "PUT"; // fs create()
const STATUS_OK: &str = "HTTP/1.1 200 Ok\r\n\r\n";
#[allow(dead_code)] // FIXME remove when implement function(s) to access fs
const STATUS_CREATED: &str = "HTTP/1.1 201 Created\r\n\r\n";
#[allow(dead_code)] // FIXME remove when implement function(s) to access fs
const STATUS_NOT_FOUND: &str = "HTTP/1.1 204 Not Found\r\n\r\n";
const STATUS_NOT_ALLOWED: &str = "HTTP/1.1 205 Not Found\r\n\r\n";

fn main() {
  let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

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
  // puts stream's bytes into a slice of bytes
  match stream.read(&mut buffer) {
    Ok(_) => {
      let req_str: &str = &String::from_utf8_lossy(&buffer[..]);
      let req_lines: Vec<&str> = req_str.lines().collect();
      let header_parts: Vec<&str> = req_lines[0].split_whitespace().collect();

      // determine the type of request
      let (status_code, contents_op) = match header_parts[0] {
        GET => {
          // TODO use function to actually access labeled fs
          (STATUS_OK, Some("dummycontents"))
        },
        POST => {
          // TODO use function to actually access labeled fs
          (STATUS_OK, None)
        },
        PUT => {
          // TODO use function to actually access labeled fs
          (STATUS_OK, None)
        },
        _ => (STATUS_NOT_ALLOWED, None),
      };
      let response = if contents_op.is_some() {
        let contents = contents_op.unwrap();
        format!(
          "{}\r\nContent-Length: {}\r\n\r\n{}",
          status_code,
          contents.len(),
          contents
        )
      } else {
        format!(
          "{}\r\nContent-Length: {}\r\n\r\n",
          status_code,
          0
        )
      };

      match stream.write(response.as_bytes()) {
        Ok(_) => {
          match stream.flush() {
            Ok(_) => println!("Response written successfully"),
            Err(e) => println!("Error flushing response: {:?}", e),
          }
        },
        Err(e) => println!("Error writing response: {:?}", e),
      };
    },
    Err(e) => println!("Error reading request: {:?}", e),
  }
}
