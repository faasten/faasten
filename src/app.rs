use http;
use bytes::Bytes;

use crate::request;

pub trait Handler {
    fn handle_request(&mut self, request: &http::Request<Bytes>) -> Result<request::Request, http::StatusCode>;
}
