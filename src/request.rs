use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub function: String,
    pub payload: Value,
}

pub fn parse_json(json: String) -> Result<Request, serde_json::Error> {
    serde_json::from_str(json.as_str())
}
