use std::sync::mpsc::Sender;
use crate::request;

#[derive(Debug)]
pub enum Message {
    Shutdown,
    Request(request::Request, Sender<Message>),
    Response(String),
}
