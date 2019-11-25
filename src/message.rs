use crate::request;

#[derive(Debug)]
pub enum Message {
    Shutdown,
    Request(request::Request),
}
