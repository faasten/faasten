use std::sync::mpsc::Sender;
use std::sync::{Mutex, Arc};
use std::net::{TcpStream};
use crate::request;

#[derive(Debug)]
pub enum Message {
    Shutdown,
    Request(request::Request, Sender<Message>),
    RequestTcp(request::Request, Arc<Mutex<TcpStream>>),
    Response(String),
}
