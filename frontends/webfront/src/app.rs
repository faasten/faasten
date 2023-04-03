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
pub struct GithubOAuthCredentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Claims {
    pub alg: String,
    pub iat: u64,
    pub exp: u64,
    pub sub: String,
}

#[derive(Clone)]
pub struct App<S: BackingStore> {
    gh_creds: GithubOAuthCredentials,
    pkey: PKey<pkey::Private>,
    pubkey: PKey<pkey::Public>,
    blobstore: Arc<Mutex<Blobstore>>,
    fs: Arc<FS<S>>,
    base_url: String,
    conn: r2d2::Pool<Scheduler>,
}

impl<S: BackingStore> App<S> {
    pub fn new(
        gh_creds: GithubOAuthCredentials,
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
            gh_creds,
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
            (GET) (/login/github) => {
                Ok(Response::redirect_302(
                    format!("https://github.com/login/oauth/authorize?client_id={}&scope=repo:invites", self.gh_creds.client_id)))
            },
            (GET) (/authenticate/github) => {
                self.auth_github(request)
            },
            /*(POST) (/pair_github) => {
                self.pair_github_to_user(request)
            },*/
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
            github: Option<String>,
        }
        let github = None; // TODO
        Ok(Response::json(&User { login, github }))
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

    fn auth_github(&self, request: &Request) -> Result<Response, Response> {
        let code = request.get_param("code").ok_or(Response::empty_404())?;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let uat = client
            .post(format!("https://github.com/login/oauth/access_token"))
            .header(reqwest::header::ACCEPT, "application/vnd.github.v3+json")
            .header(reqwest::header::USER_AGENT, "SnapFaaS Web Frontend")
            .multipart(
                reqwest::blocking::multipart::Form::new()
                    .text("client_id", self.gh_creds.client_id.clone())
                    .text("client_secret", self.gh_creds.client_secret.clone())
                    .text("code", code),
            )
            .send()
            .expect("reqwest");

        #[derive(Debug, Deserialize)]
        struct AuthResponse {
            access_token: String,
        }
        let t: AuthResponse = uat.json().map_err(|_| Response::empty_400())?;
        Ok(Response::html(format!(
            include_str!("authenticated_github.html"),
            token = t.access_token,
            base_url = self.base_url
        )))
    }

    /*fn pair_github_to_user(&self, request: &Request) -> Result<Response, Response> {
        let local_user = self.verify_jwt(request)?;

        let input = rouille::post_input!(request, {
            github_token: String,
        })
        .map_err(|e| {
            println!("{:?}", e);
            Response::empty_400()
        })?;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let github_user: github_types::User = client
            .get(format!("https://api.github.com/user"))
            .header(reqwest::header::ACCEPT, "application/vnd.github.v3+json")
            .header(reqwest::header::USER_AGENT, "SnapFaaS Web Frontend")
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", input.github_token),
            )
            .send()
            .expect("reqwest")
            .json()
            .unwrap();
        let mut txn = self.dbenv.begin_rw_txn().unwrap();
        txn.put(
            *self.default_db,
            &format!("users/github/for/user/{}", &local_user).as_str(),
            &github_user.login.as_str(),
            lmdb::WriteFlags::empty(),
        )
        .expect("store user");
        txn.put(
            *self.default_db,
            &format!("users/github/user/{}/token", &github_user.login).as_str(),
            &input.github_token.as_str(),
            lmdb::WriteFlags::empty(),
        )
        .expect("store user");
        txn.put(
            *self.default_db,
            &format!("users/github/from/{}", &github_user.login).as_str(),
            &local_user.as_str(),
            lmdb::WriteFlags::empty(),
        )
        .expect("store user");
        txn.commit().expect("commit");
        Ok(Response::json(&github_user.login))
    }*/
}
