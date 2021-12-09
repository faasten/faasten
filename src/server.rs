use bytes::{BufMut, Bytes, BytesMut};
use http;
use httparse;
use log::trace;

use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::mpsc::Sender;

use crate::app::Handler;
use crate::workerpool::WorkerPool;
use crate::message::Message;

pub struct Client {
    stream: TcpStream,
}

impl Client {
    pub fn new(stream: TcpStream) -> Client {
        Client { stream }
    }

    pub fn read_request(
        &mut self,
        buf: &mut BytesMut,
    ) -> Result<(http::request::Builder, BytesMut), std::io::Error> {
        loop {
            let mut lowbuf = [0u8; 2048];
            let len = self.stream.read(&mut lowbuf)?;
            if len == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Connection closed",
                ));
            }
            buf.put(&lowbuf[..len]);
            let mut headers = [httparse::EMPTY_HEADER; 100];
            let mut req = httparse::Request::new(&mut headers);
            let res = req.parse(buf.as_ref()).unwrap();
            if let httparse::Status::Complete(len) = res {
                let method = req
                    .method
                    .and_then(|method| http::method::Method::from_bytes(method.as_bytes()).ok())
                    .unwrap();
                let uri = http::uri::Uri::builder()
                    .path_and_query(req.path.unwrap_or(""))
                    .build()
                    .unwrap();
                let version = http::version::Version::HTTP_11; // TODO

                let mut result = http::request::Builder::new()
                    .method(method)
                    .uri(uri)
                    .version(version);

                for header in req.headers.iter() {
                    let name =
                        http::header::HeaderName::from_bytes(header.name.as_bytes()).unwrap();
                    let value = http::header::HeaderValue::from_bytes(header.value).unwrap();
                    result = result.header(name, value);
                }
                return Ok((result, buf.split_off(len)));
            }
        }
    }

    pub fn read(&mut self) -> Result<http::Request<Bytes>, std::io::Error> {
        let (request, mut buf) = self.read_request(&mut BytesMut::with_capacity(2048))?;
        buf = buf.split();
        if let Some(length) = request.headers_ref().and_then(|headers| {
            headers
                .get("content-length")
                .and_then(|cl| String::from_utf8_lossy(cl.as_bytes()).parse::<usize>().ok())
        }) {
            let mut remaining = BytesMut::with_capacity(length - buf.len());
            remaining.resize(length - buf.len(), 0);
            self.stream.read_exact(remaining.as_mut())?;
            buf.unsplit(remaining);
        }
        let result = request.body(buf.freeze()).unwrap();
        Ok(result)
    }

    pub fn write_response<B: AsRef<[u8]>>(
        &mut self,
        response: &http::Response<B>,
    ) -> Result<(), std::io::Error> {
        let body = response.body().as_ref();
        let status = response.status();
        write!(
            self.stream,
            "HTTP/1.1 {} {}\r\n",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Err")
        )?;
        for (name, value) in response.headers().iter() {
            write!(self.stream, "{}: ", name.as_str())?;
            self.stream.write_all(value.as_bytes())?;
            write!(self.stream, "\r\n")?;
        }

        write!(self.stream, "Content-Length: {}\r\n", body.len())?;
        write!(self.stream, "\r\n")?;
        self.stream.write_all(body)
    }
}

pub struct Server<H> {
    wp: WorkerPool,
    listener: TcpListener,
    handler: H,
}

impl<H> Server<H> {
    pub fn new(total_mem: usize, config_path: &str, listen_addr: &str, handler: H) -> Self {
        use crate::{configs, controller, workerpool};
        // create a controller object
        let config = configs::ControllerConfig::new(config_path);
        let mut controller = controller::Controller::new(config);
        controller.set_total_mem(total_mem);
        let controller = std::sync::Arc::new(controller);
        trace!("{:?}", controller);

        let listener = TcpListener::bind(listen_addr).unwrap();
        trace!("listening on {}", listen_addr);

        // worker pool
        let wp = workerpool::WorkerPool::new(controller.clone());
        trace!("# workers: {:?}", wp.pool_size());

        Server { wp, listener, handler }
    }
}

impl<H: 'static + Handler + Send + Clone> Server<H> {
    pub fn run(&self) -> Result<(), std::io::Error> {
        for stream in self.listener.incoming() {
            let stream = stream?;
            let mut handler = self.handler.clone();
            let sender = self.wp.get_sender();
            std::thread::spawn(move || {
                let mut client = Client::new(stream);
                loop {
                    if let Err(r) = request_helper(&mut client, &mut handler, &sender) {
                        if r.kind() != std::io::ErrorKind::UnexpectedEof {
                            eprintln!("{}", r);
                        }
                        break;
                    }
                }
            });
        }
        Ok(())
    }
}

fn request_helper(client: &mut Client, handler: &mut dyn Handler, sender: &Sender<Message>) -> Result<(), std::io::Error> {
    use http::Response;
    let request = client.read()?;

    match handler.handle_request(&request) {
        Ok(req) => {
            let (tx, rx) = mpsc::channel();
            sender.send(Message::HTTPRequest(req, tx))
                .expect("failed to send HTTP-sourced request");
            
            match rx.recv().expect("failed to receive worker response") {
                Ok(_) => client.write_response(&Response::builder().body(request.body().clone()).unwrap()),
                Err(c) => client.write_response(&Response::builder().status(c).body(Bytes::new()).unwrap()),
            }
        },
        Err(c) => client.write_response(&Response::builder().status(c).body(Bytes::new()).unwrap()),
    }
}
