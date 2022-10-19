use bytes::Bytes;
use http;
use http::status::StatusCode;
use log::{error, debug};

use std::net::TcpStream;

use snapfaas::request;

use httpserver::Handler;

struct SnapFaasManager {
    address: String,
}

impl r2d2::ManageConnection for SnapFaasManager {
    type Connection = TcpStream;
    type Error = std::io::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(TcpStream::connect(&self.address)?)
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        let req = request::Request {
            function: String::from("ping"),
            payload: serde_json::Value::Null,
            label: labeled::dclabel::DCLabel::public(),
            data_handles: Default::default(),
        };
        request::write_u8(&req.to_vec(), conn)?;
        request::read_u8(conn)?;
        Ok(())
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.take_error().ok().flatten().is_some()
    }
}

#[derive(Clone)]
pub struct App {
    conn: r2d2::Pool<SnapFaasManager>,
    secret: Option<String>,
}

impl App {
    pub fn new(secret: Option<String>, snapfaas_address: String) -> Self {
        let conn = r2d2::Pool::builder().max_size(10).build(SnapFaasManager { address: snapfaas_address }).expect("pool");
        App {
            secret,
            conn,
        }
    }

    pub fn handle_github_event(&self, request: &http::Request<Bytes>) -> AppResult<Bytes> {
        let event_type = request
            .headers()
            .get("x-github-event")
            .ok_or(StatusCode::BAD_REQUEST)?;
        debug!("Headers contain x-github-event key.");
        let etype = event_type.to_str().or(Err(StatusCode::BAD_REQUEST))?;
        match etype {
            "ping" => {
                verify_github_request(
                    &self.secret,
                    &request.body(),
                    request
                        .headers()
                        .get("x-hub-signature")
                        .map(|v| v.as_bytes()),
                )?;

                debug!("GitHub pinged.");
                Ok(Bytes::new())
            }
            _ => {
                debug!("{} event.", etype);
                verify_github_request(
                    &self.secret,
                    &request.body(),
                    request
                        .headers()
                        .get("x-hub-signature")
                        .map(|v| v.as_bytes()),
                )?;

                // TODO: use the event body to set a label?
                /*let event_body: PushEvent =
                    serde_json::from_slice(request.body().as_ref()).or(Err(StatusCode::BAD_REQUEST))?;*/
                let mut event_body: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_slice(request.body().as_ref()).or(Err(StatusCode::BAD_REQUEST))?;
                event_body.insert(String::from("event"), etype.into());

                let req = request::Request {
                    function: "gh_repo".to_string(),
                    payload: event_body.into(),
                    label: labeled::dclabel::DCLabel::public(),
                    data_handles: Default::default(),
                };

                let conn = &mut self.conn.get().expect("Lock failed");
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
                            request::RequestStatus::SentToVM(response) => Ok(Bytes::from(response)),
                            request::RequestStatus::ProcessRequestFailed => Err(StatusCode::INTERNAL_SERVER_ERROR),
                        }
                    },
                }
            },
        }
    }
}

type AppResult<T> = Result<T, StatusCode>;

impl Handler for App {
    fn handle_request(&mut self, request: &http::Request<Bytes>) -> http::Response<Bytes> {
        match self.handle_github_event(request) {
            Ok(body) => http::Response::builder()
                .body(body)
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
