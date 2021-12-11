use github_types::PushEvent;
use bytes::Bytes;
use http::status::StatusCode;
use serde::Serialize;
use log::debug;

use std::time::Instant;

use snapfaas::request;
use snapfaas::app;

use crate::config::Config;

#[derive(Serialize)]
struct Payload {
    pub tarball: Vec<u8>,
}

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

    // download the tarball of a repository from GitHub
    fn get_github_tarball(&self, event_body: &PushEvent) -> Vec<u8> {
        use url::Url;
        let tarball_url = format!("https://api.github.com/repos/{}/tarball/{}",
                                  &event_body.repository.full_name, &event_body.after);
        let tarball_url = Url::parse(&tarball_url).unwrap();
        debug!("Archive URL: {:?}", tarball_url);
        
        use curl::easy::{Easy, List};
        let mut easy = Easy::new();
        easy.url(tarball_url.as_str()).unwrap();
        easy.useragent("webhook-snapfaas").unwrap();
        if let Some(token) = self.config.repos[&event_body.repository.full_name].token.as_ref() {
            let mut headers = List::new();
            headers.append(format!("Authorization: token {}", token).as_str()).unwrap();
            easy.http_headers(headers).unwrap();
        }
        easy.follow_location(true).unwrap();

        let mut buf = Vec::new();
        {
            let mut transfer = easy.transfer();
            transfer.write_function(|data| {
                buf.extend_from_slice(data);
                Ok(data.len())
            }).unwrap();
            transfer.perform().unwrap();
        }

        {
            use log::{log_enabled, Level};
            if log_enabled!(Level::Debug) {
                use flate2::read::GzDecoder;
                use tar::Archive;
                let tar = GzDecoder::new(&buf[..]);
                let mut archive = Archive::new(tar);
                if let Ok(entries) = archive.entries() {
                    for entry in entries {
                        match entry {
                            Ok(ent) => debug!("{:?}", ent.path()),
                            Err(err) => debug!("Invalid Entry: {:?}", err),
                        }
                    }
                } else {
                    debug!("Bad gzip format: {:?}", archive.entries().err().unwrap());
                }
            }
        }

        buf
    }
}

type AppResult<T> = Result<T, StatusCode>;

impl app::Handler for App {
    fn handle_request(&mut self, request: &http::Request<Bytes>) -> AppResult<Option<request::Request>> {
        let event_type = request
            .headers()
            .get("x-github-event")
            .ok_or(StatusCode::BAD_REQUEST)?;
        match event_type.as_bytes() {
            b"ping" => {
                debug!("GitHub pinged.");
                Ok(None)
            }
            b"push" => {
                debug!("Push event.");
                let event_body: PushEvent =
                    serde_yaml::from_slice(request.body().as_ref()).or(Err(StatusCode::BAD_REQUEST))?;

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

                let tarball = self.get_github_tarball(&event_body);

                use std::hash::{Hash, Hasher};
                let mut s = std::collections::hash_map::DefaultHasher::new();
                event_body.repository.full_name.hash(&mut s);
                Ok(Some(request::Request {
                    time: Instant::now().duration_since(self.uptime).as_millis() as u64,
                    user_id: s.finish(),
                    function: "build_tarball".to_string(),
                    payload: serde_json::to_value(Payload{ tarball }).unwrap(),
                }))
            },
            _ => Err(StatusCode::BAD_REQUEST)
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
                Err(StatusCode::BAD_REQUEST)
            }
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    } else {
        Ok(())
    }
}
