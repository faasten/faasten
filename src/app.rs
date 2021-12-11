use http;
use bytes::Bytes;

use crate::request;

pub trait Handler {
    fn handle_request(&mut self, request: &http::Request<Bytes>) -> Result<Option<request::Request>, http::StatusCode>;
}
