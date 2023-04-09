use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;

use jwt::{PKeyWithDigest, SignWithKey, VerifyWithKey};
use log::error;
use openssl::pkey::{self, PKey};
use reqwest::blocking::Client;
use rouille::{Request, Response};
use serde::{Deserialize, Serialize};

use snapfaas::blobstore::Blobstore;
use snapfaas::fs::BackingStore;
use snapfaas::fs::FS;
use snapfaas::sched;
use snapfaas::sched::Scheduler;

#[derive(Clone)]
#[derive(Serialize, Deserialize, Debug)]
struct Claims {
    pub alg: String,
    pub iat: u64,
    pub exp: u64,
    pub sub: String,
}

#[derive(Clone)]
pub struct App<S: BackingStore> {
    pkey: PKey<pkey::Private>,
    pubkey: PKey<pkey::Public>,
    blobstore: Arc<Mutex<Blobstore>>,
    fs: Arc<FS<S>>,
    base_url: String,
    conn: r2d2::Pool<Scheduler>,
}

impl<S: BackingStore> App<S> {
    pub fn new(
        pkey: PKey<pkey::Private>,
        pubkey: PKey<pkey::Public>,
        blobstore: Blobstore,
        fs: FS<S>,
        base_url: String,
        addr: String,
    ) -> App<S> {
        let conn = r2d2::Pool::builder()
            .max_size(10)
            .build(Scheduler::new(&addr))
            .expect("pool");
        let blobstore = Arc::new(Mutex::new(blobstore));
        App {
            conn,
            blobstore,
            fs: Arc::new(fs),
            pkey,
            pubkey,
            base_url,
        }
    }

    fn verify_jwt(&self, request: &Request) -> Result<String, Response> {
        let jwt = request
            .header("Authorization")
            .and_then(|header| header.split(" ").last())
            .ok_or(Response::empty_400())?;
        let key = PKeyWithDigest {
            key: self.pubkey.clone(),
            digest: openssl::hash::MessageDigest::sha256(),
        };
        let claims: Claims = jwt
            .verify_with_key(&key)
            .map_err(|e| e)
            .map_err(|_| Response::empty_400())?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if claims.exp < now && false {
            // TODO: remove false for production
            Err(Response::json(&serde_json::json!({
                "error": "Authentication token expired"
            }))
            .with_status_code(403))
        } else {
            Ok(claims.sub)
        }
    }

    pub fn handle(&mut self, request: &Request) -> Response {
        if request.method().to_uppercase().as_str() == "OPTIONS" {
            return Response::empty_204()
                .with_additional_header("Access-Control-Allow-Origin", "*")
                .with_additional_header(
                    "Access-Control-Allow-Headers",
                    "Authorization, Content-type",
                )
                .with_additional_header("Access-Control-Allow-Methods", "*");
        }
        rouille::router!(request,
            (GET) (/login/cas) => {
                Ok(Response::redirect_302(
                    format!("{}/login?service={}", "https://fed.princeton.edu/cas", format!("{}/authenticate/cas", self.base_url))))
            },
            (GET) (/authenticate/cas) => {
                self.authenticate_cas(request)
            },
            (GET) (/me) => {
                self.whoami(request)
            },
            (GET) (/faasten/ping) => {
                Ok(Response::text("Pong.").with_status_code(200))
            },
            (GET) (/faasten/ping/scheduler) => {
                self.faasten_ping_scheduler()
            },
            (POST) (/faasten/invoke/{gate_path}) => {
                self.faasten_invoke(gate_path, request)
            },
            _ => {
                error!("404: {} {}", request.method(), request.raw_url());
                Ok(Response::empty_404())
            }
        ).unwrap_or_else(|e| e).with_additional_header("Access-Control-Allow-Origin", "*")
    }

    fn faasten_invoke(&self, gate_path: String, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request).ok();

        let conn = &mut self.conn.get().map_err(|_| {
            Response::json(&serde_json::json!({
                "error": "failed to get scheduler connection"
            }))
            .with_status_code(500)
        })?;

        super::init::init(
            login,
            gate_path,
            request,
            conn,
            self.fs.as_ref(),
            self.blobstore.clone(),
        )
    }

    // check if we can reach the scheduler
    fn faasten_ping_scheduler(&self) -> Result<Response, Response> {
        let conn = &mut self.conn.get().map_err(|_| {
            Response::json(&serde_json::json!({
                "error": "failed to get scheduler connection"
            }))
            .with_status_code(500)
        })?;

        sched::rpc::ping(conn)
            .map_err(|_| {
                Response::json(&serde_json::json!({
                    "error": "failed to ping faasten scheduler"
                }))
                .with_status_code(500)
            })
            .map(|_| Response::empty_204())
    }

    fn whoami(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;
        #[derive(Serialize)]
        struct User {
            login: String,
        }
        Ok(Response::json(&User { login }))
    }

    fn authenticate_cas(&self, request: &Request) -> Result<Response, Response> {
        let ticket = request.get_param("ticket").ok_or(Response::empty_404())?;
        let service = format!("{}/authenticate/cas", self.base_url);

        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let validate_cas = client
            .get(format!("{}/validate", "https://fed.princeton.edu/cas"))
            .query(&[("ticket", ticket), ("service", service)])
            .send()
            .expect("reqwest");
        let sub = validate_cas
            .text()
            .or(Err(Response::empty_400()))
            .and_then(|text| {
                let result: Vec<&str> = text.lines().collect();
                match result.as_slice() {
                    // FIXME buckle parser does not allow `@`. should we?
                    ["yes", user] => Ok(format!("{}", user)),
                    _ => Err(Response::empty_400()),
                }
            })?;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            alg: "ES256".to_string(),
            iat: now,
            exp: now + 10 * 60,
            sub: sub.clone(),
        };
        let key = PKeyWithDigest {
            key: self.pkey.clone(),
            digest: openssl::hash::MessageDigest::sha256(),
        };
        let token = claims.sign_with_key(&key).unwrap();

        snapfaas::fs::bootstrap::register_user_fsutil(self.fs.as_ref(), sub);

        Ok(Response::html(format!(
            include_str!("authenticated_cas.html"),
            token
        )))
    }
}
