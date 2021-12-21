use github_types::{PushEvent, PingEvent};
use bytes::Bytes;
use http;
use http::status::StatusCode;
use log::{error, debug};

use std::time::Instant;
use std::net::TcpStream;

use snapfaas::request;

use crate::config::Config;
use crate::server::Handler;

#[derive(Clone)]
pub struct App {
    config: Config,
    uptime: Instant,
}

impl App {
    pub fn new(config_path: &str) -> Self {
        let config = Config::new(config_path);
        debug!("App config: {:?}", config);
        App {
            config,
            uptime : Instant::now(),
        }
    }

    pub fn handle_github_event(&self, request: &http::Request<Bytes>, conn: &mut TcpStream) -> AppResult<Bytes> {
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
                Ok(Bytes::new())
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

                use std::hash::{Hash, Hasher};
                let mut s = std::collections::hash_map::DefaultHasher::new();
                event_body.repository.full_name.hash(&mut s);
                let req = request::Request {
                    time: Instant::now().duration_since(self.uptime).as_millis() as u64,
                    user_id: s.finish(),
                    function: "build_tarball".to_string(),
                    payload: serde_json::from_slice(request.body()).unwrap(),
                };

                if let Err(e) = request::write_u8(serde_json::to_string(&req).unwrap().as_bytes(), conn) {
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
                        debug!("Reponse for user_id: {:?}", rsp.user_id);
                        Ok(Bytes::from(serde_json::to_vec(&rsp.payload).unwrap()))
                    },
                }
            },
            _ => Err(StatusCode::BAD_REQUEST),
        }
    }

    //// download the tarball of a repository from GitHub
    //fn get_github_tarball(&self, event_body: &PushEvent) -> Vec<u8> {
    //    use url::Url;
    //    let tarball_url = format!("https://api.github.com/repos/{}/tarball/{}",
    //                              &event_body.repository.full_name, &event_body.after);
    //    let tarball_url = Url::parse(&tarball_url).unwrap();
    //    debug!("Archive URL: {:?}", tarball_url);
    //    
    //    use curl::easy::{Easy, List};
    //    let mut easy = Easy::new();
    //    easy.url(tarball_url.as_str()).unwrap();
    //    easy.useragent("webhook-snapfaas").unwrap();
    //    if let Some(token) = self.config.repos[&event_body.repository.full_name].token.as_ref() {
    //        let mut headers = List::new();
    //        headers.append(format!("Authorization: token {}", token).as_str()).unwrap();
    //        easy.http_headers(headers).unwrap();
    //    }
    //    easy.follow_location(true).unwrap();

    //    let mut buf = Vec::new();
    //    {
    //        let mut transfer = easy.transfer();
    //        transfer.write_function(|data| {
    //            buf.extend_from_slice(data);
    //            Ok(data.len())
    //        }).unwrap();
    //        transfer.perform().unwrap();
    //    }

    //    {
    //        use log::{log_enabled, Level};
    //        if log_enabled!(Level::Debug) {
    //            use flate2::read::GzDecoder;
    //            use tar::Archive;
    //            let tar = GzDecoder::new(&buf[..]);
    //            let mut archive = Archive::new(tar);
    //            if let Ok(entries) = archive.entries() {
    //                for entry in entries {
    //                    match entry {
    //                        Ok(ent) => debug!("{:?}", ent.path()),
    //                        Err(err) => debug!("Invalid Entry: {:?}", err),
    //                    }
    //                }
    //            } else {
    //                debug!("Bad gzip format: {:?}", archive.entries().err().unwrap());
    //            }
    //        }
    //    }

    //    buf
    //}
}

type AppResult<T> = Result<T, StatusCode>;

impl Handler for App {
    fn handle_request(&mut self, request: &http::Request<Bytes>, conn: &mut TcpStream) -> http::Response<Bytes> {
        match self.handle_github_event(request, conn) {
            Ok(buf) => http::Response::builder()
                .body(buf)
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
