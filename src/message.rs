use http::StatusCode;

use std::sync::mpsc::Sender;
use std::sync::{Mutex, Arc};
use std::net::{TcpStream};

use crate::request::Request;

#[derive(Debug)]
pub enum Message {
    HTTPRequest(Request, Sender<Result<(), StatusCode>>),
    Shutdown(Sender<Message>),
    ShutdownAck,
    NoAckShutdown,
    Request(Request, Sender<Message>),
    RequestTcp(Request, Arc<Mutex<TcpStream>>),
    Response(String),
}
