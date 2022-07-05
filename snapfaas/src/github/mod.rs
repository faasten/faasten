use reqwest;
use labeled::dclabel::{DCLabel, Component};
use labeled::Label;
use crate::syscalls;

const GITHUB_REST_ENDPOINT: &str = "https://api.github.com";
const GITHUB_REST_API_VERSION_HEADER: &str = "application/json+vnd";
const GITHUB_AUTH_TOKEN: &str = "GITHUB_AUTH_TOKEN";
const USER_AGENT: &str = "snapfaas";

#[derive(Debug)]
pub enum Error {
    Unauthorized,
    HttpReq(reqwest::Error),
    BadHttpVerb,
    EmptyBody,
    BadRoute,
}

type Result<T> = std::result::Result<T, Error>;

fn check_label(route: &str, verb: syscalls::HttpVerb, cur_label: &mut DCLabel) -> Result<()> {
    if let Some(suffix) = route.strip_prefix("/repos") {
        let v: Vec<&str> = suffix.split('/').collect();
        if v.len() < 2 {
            return Err(Error::BadRoute)
        }
        let secrecy = format!("{}-{}-github", v[0], v[1]);
        if verb == syscalls::HttpVerb::Get {
            let gh_read = DCLabel::new([[secrecy]], [["github"]]);
            if !gh_read.can_flow_to(cur_label) {
                *cur_label = gh_read.clone().lub(cur_label.clone());
            }
        } else if verb == syscalls::HttpVerb::Post && suffix.ends_with("generate") {
            let integrity = format!("{}-github", v[0]);
            let gh_write = DCLabel::new([[secrecy]], [[integrity]]);
            if !cur_label.can_flow_to(&gh_write) {
                return Err(Error::Unauthorized);
            }
        } else {
            let integrity = format!("{}-{}-github", v[0], v[1]);
            let gh_write = DCLabel::new([[secrecy]], [[integrity]]);
            if !cur_label.can_flow_to(&gh_write) {
                return Err(Error::Unauthorized);
            }
        }
        return Ok(());
    }
    Err(Error::BadRoute)
}

fn scverb_to_reqwestverb(verb: syscalls::HttpVerb) -> reqwest::Method {
    match verb {
        syscalls::HttpVerb::Get => reqwest::Method::GET,
        syscalls::HttpVerb::Post => reqwest::Method::POST,
        syscalls::HttpVerb::Put => reqwest::Method::PUT,
        syscalls::HttpVerb::Delete => reqwest::Method::DELETE,
    }
}

#[derive(Debug)]
pub struct Client {
    conn: reqwest::blocking::Client,
}

impl Client {
    pub fn new() -> Self {
        Self { conn: reqwest::blocking::Client::new() }
    }

    /// process requests to Github REST API
    pub fn process(&self, req: &syscalls::GithubRest, cur_label: &mut DCLabel, privilege: &Component) -> Result<reqwest::blocking::Response> {
        syscalls::HttpVerb::from_i32(req.verb).map_or_else(
            || Err(Error::BadHttpVerb),
            |verb| {
                if verb != syscalls::HttpVerb::Get {
                    *cur_label = cur_label.endorse(privilege);
                }
                check_label(&req.route, verb, cur_label)?;
                self.http(&req.route, verb, req.body, &req.token)
            })
    }

    // send out HTTP requests
    fn http(&self, route: &str, verb: syscalls::HttpVerb, body: Option<String>, token: &str) -> Result<reqwest::blocking::Response> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(route);
        let method = scverb_to_reqwestverb(verb);
        let mut http_req = self.conn.request(method, url)
            .header(reqwest::header::ACCEPT, GITHUB_REST_API_VERSION_HEADER)
            .header(reqwest::header::USER_AGENT, USER_AGENT);
        if method != reqwest::Method::GET {
            http_req = http_req.body(std::string::String::from(body.as_ref().ok_or(Error::EmptyBody)?));
        }
        http_req.bearer_auth(token).send().map_err(|e| Error::HttpReq(e))
    }
}
