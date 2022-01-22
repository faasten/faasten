use github_types::{PushEvent, PingEvent};
use bytes::Bytes;
use http;
use http::status::StatusCode;
use log::{error, debug};

use std::sync::Arc;
use std::sync::Mutex;
use std::net::TcpStream;

use snapfaas::request;

use crate::config::Config;
use crate::server::Handler;

#[derive(Clone)]
pub struct App {
    conn: Arc<Mutex<TcpStream>>,
    config: Config,
}

impl App {
    pub fn new(config_path: &str, snapfaas_address: String) -> Self {
        let config = Config::new(config_path);
        debug!("App config: {:?}", config);
        let conn = Arc::new(Mutex::new(TcpStream::connect(snapfaas_address).expect("Cannot connect to snapfaas")));
        App {
            config,
            conn,
        }
    }

    pub fn handle_github_event(&self, request: &http::Request<Bytes>) -> AppResult<()> {
        let event_type = request
            .headers()
            .get("x-github-event")
            .ok_or(StatusCode::BAD_REQUEST)?;
        debug!("Headers contain x-github-event key.");
        match event_type.as_bytes() {
            b"ping" => {
                let event_body: PingEvent =
                    serde_json::from_slice(request.body().as_ref()).or(Err(StatusCode::BAD_REQUEST))?;

                let name = &event_body.repository.ok_or(StatusCode::BAD_REQUEST)?.full_name;
                let repo = self
                    .config
                    .repos
                    .get(name)
                    .ok_or(StatusCode::NOT_FOUND)?;
                verify_github_request(
                    &repo.secret,
                    &request.body(),
                    request
                        .headers()
                        .get("x-hub-signature")
                        .map(|v| v.as_bytes()),
                )?;

                debug!("GitHub pinged.");
                Ok(())
            }
            b"push" => {
                debug!("Push event.");
                let event_body: PushEvent =
                    serde_json::from_slice(request.body().as_ref()).or(Err(StatusCode::BAD_REQUEST))?;

                let name = &event_body.repository.full_name;
                let repo = self
                    .config
                    .repos
                    .get(name)
                    .ok_or(StatusCode::NOT_FOUND)?;
                verify_github_request(
                    &repo.secret,
                    &request.body(),
                    request
                        .headers()
                        .get("x-hub-signature")
                        .map(|v| v.as_bytes()),
                )?;

                let req = request::Request {
                    function: "build_tarball".to_string(),
                    payload: serde_json::from_slice(request.body()).unwrap(),
                };

                let conn = &mut *self.conn.lock().expect("Lock failed");
                if let Err(e) = request::write_u8(&req.to_vec(), conn) {
                    error!("Failed to send request to snapfaas: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }

                match request::read_u8(conn) {
                    Err(e) => {
                        error!("Failed to read response from snapfaas: {:?}", e);
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    },
                    Ok(buf) => {
                        let rsp: request::Response = serde_json::from_slice(&buf).unwrap();
                        debug!("Reponse {:?}", rsp);
                        match rsp.status {
                            request::RequestStatus::ResourceExhausted => Err(StatusCode::TOO_MANY_REQUESTS),
                            request::RequestStatus::FunctionNotExist | request::RequestStatus::Dropped => Err(StatusCode::BAD_REQUEST),
                            request::RequestStatus::LaunchFailed => Err(StatusCode::INTERNAL_SERVER_ERROR),
                            request::RequestStatus::SentToVM => Ok(()),
                        }
                    },
                }
            },
            _ => Err(StatusCode::BAD_REQUEST),
        }
    }
}

type AppResult<T> = Result<T, StatusCode>;

impl Handler for App {
    fn handle_request(&mut self, request: &http::Request<Bytes>) -> http::Response<Bytes> {
        match self.handle_github_event(request) {
            Ok(()) => http::Response::builder()
                .body(Bytes::new())
                .unwrap(),
            Err(status_code) => http::Response::builder()
                .status(status_code)
                .body(Bytes::new())
                .unwrap(),
        }
    }
}

fn from_hex(hex_str: &str) -> AppResult<Vec<u8>> {
    fn from_digit(digit: u8) -> AppResult<u8> {
        match digit {
            b'0'..=b'9' => Ok(digit - b'0'),
            b'A'..=b'F' => Ok(10 + digit - b'A'),
            b'a'..=b'f' => Ok(10 + digit - b'a'),
            _ => return Err(StatusCode::BAD_REQUEST),
        }
    }

    if hex_str.len() & 1 != 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut result = Vec::with_capacity(hex_str.len() / 2);
    for digits in hex_str.as_bytes().chunks(2) {
        let hi = from_digit(digits[0])?;
        let lo = from_digit(digits[1])?;
        result.push((hi << 4) | lo);
    }
    Ok(result)
}

fn verify_github_request(
    secret: &Option<String>,
    payload: &Bytes,
    tag: Option<&[u8]>,
) -> AppResult<()> {
    use ring::hmac;
    if let Some(secret) = secret {
        if let Some(tag) = tag {
            let tag = String::from_utf8_lossy(tag);
            let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret.as_bytes());
            let tagbytes = from_hex(&tag[5..])?;
            if tag.starts_with("sha1=") {
                hmac::verify(&key, payload.as_ref(), tagbytes.as_slice())
                    .or(Err(StatusCode::UNAUTHORIZED))
            } else {
                debug!("Verification failed.");
                Err(StatusCode::BAD_REQUEST)
            }
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    } else {
        Ok(())
    }
}
