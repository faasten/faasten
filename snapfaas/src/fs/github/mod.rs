use std::sync::RwLock;

use lazy_static::lazy_static;
use reqwest;
use route_recognizer::{Router, Params};
use labeled::buckle::{Buckle, Component, Clause};
use labeled::Label;
use crate::syscalls;

const GITHUB_REST_ENDPOINT: &str = "https://api.github.com";
const GITHUB_REST_API_VERSION_HEADER: &str = "application/json+vnd";
const USER_AGENT: &str = "snapfaas";

#[derive(Debug)]
pub enum Error {
    Unauthorized,
    HttpReq(reqwest::Error),
    BadHttpVerb,
    EmptyBody,
    BadRoute,
    BadPayload,
}

type Result<T> = std::result::Result<T, Error>;
type label_check_fn = fn (&Params) -> Result<()>;

lazy_static!{
    // dumb router
    pub static ref ROUTER: RwLock<Router<fn (&Params) -> Result<()>>> = {
        let mut r = Router::new();
        r.add("/repos/:owner/:user/tarball/:ref", read_check as label_check_fn);
        r.add("/repos/:owner/:repo/commits/:commit_sha/comments", write_check as label_check_fn);
        r.add("/repos/:template_owner/:template_repo/generate", read_write_check as label_check_fn);
        r.add("/repos/:owner/:repo/collaborators/:user", write_check as label_check_fn);
        RwLock::new(r)
    };
}

fn read_check(params: &Params) -> Result<()> {
    Ok(())
}

fn write_check(params: &Params) -> Result<()> {
    Ok(())
}

fn read_write_check(params: &Params) -> Result<()> {
    Ok(())
}

fn check_label(req: &syscalls::GithubRest, verb: syscalls::HttpVerb) -> Result<()> {
    if let Some(suffix) = req.route.strip_prefix("/repos") {
        let v: Vec<&str> = suffix.split('/').collect();
        if v.len() < 2 {
            return Err(Error::BadRoute)
        }
        let sclause = v.splice
        if verb == syscalls::HttpVerb::Get {
            let integrity = format!("{}:{}@github", v[0], v[1]);
            let gh_read = Buckle::new([[secrecy]], [[integrity]]);
            if !gh_read.can_flow_to(cur_label) {
                *cur_label = gh_read.clone().lub(cur_label.clone());
            }
        } else if verb == syscalls::HttpVerb::Post && suffix.ends_with("generate") {
            // inspect the request body to find the owner of the repository to be created
            let val: serde_json::Value = serde_json::from_str(req.body.as_ref().ok_or(Error::EmptyBody)?)
                .map_err(|_| Error::BadPayload)?;
            let obj = val.as_object().ok_or(Error::BadPayload)?;
            let owner = obj["owner"].as_str().map(String::from).ok_or(Error::BadPayload)?;
            let secrecy = format!("{}@github", owner);
            let integrity = format!("{}@github", owner);
            let gh_write = Buckle::new([[secrecy]], [[integrity]]);
            if !cur_label.can_flow_to(&gh_write) {
                return Err(Error::Unauthorized);
            }
        } else {
            let integrity = format!("{}:{}@github", v[0], v[1]);
            let gh_write = Buckle::new([[secrecy]], [[integrity]]);
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
    address: reqwest::Url,
}

impl Client {
    pub fn new() -> Self {
        Self {
            conn: reqwest::blocking::Client::new(),
            address: std::env::var("GITHUB_MOCK").map_or_else(|_| reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap(),
                |a| reqwest::Url::parse(&a).expect("Bad GitHub URL")),
        }
    }

    /// process requests to Github REST API
    pub fn process(&self, req: syscalls::GithubRest) -> Result<reqwest::blocking::Response> {
        super::CURRENT_LABEL.with(|cur_label| {
        syscalls::HttpVerb::from_i32(req.verb).map_or_else(
            || Err(Error::BadHttpVerb),
            |verb| {
                if verb != syscalls::HttpVerb::Get {
                    super::PRIVILEGE.with(|opriv| {
                        *cur_label.borrow_mut() = Buckle::new(cur_label.borrow().secrecy.clone(),
                        cur_label.borrow().clone().integrity & opriv.borrow().clone());
                    });
                }
                let mtch = ROUTER.read().unwrap().recognize(&req.route).map_err(|_| Error::BadRoute)?;
                mtch.handler()(mtch.params())?;
                self.http(&req.route, verb, req.body, &req.token)
            })
        })
    }

    // send out HTTP requests
    fn http(&self, route: &str, verb: syscalls::HttpVerb, body: Option<String>, token: &str) -> Result<reqwest::blocking::Response> {
        let mut url = self.address.clone();
        url.set_path(route);
        let method = scverb_to_reqwestverb(verb);
        let mut http_req = self.conn.request(method.clone(), url)
            .header(reqwest::header::ACCEPT, GITHUB_REST_API_VERSION_HEADER)
            .header(reqwest::header::USER_AGENT, USER_AGENT);
        if method != reqwest::Method::GET {
            http_req = http_req.body(std::string::String::from(body.as_ref().ok_or(Error::EmptyBody)?));
        }
        http_req.bearer_auth(token).send().map_err(|e| Error::HttpReq(e))
    }
}
