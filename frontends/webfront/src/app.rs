use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::SystemTime;
use std::io::Read;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use rouille::{Request, Response};
use jwt::{PKeyWithDigest, SignWithKey, VerifyWithKey};
use openssl::pkey::{self, PKey};
use lmdb::{Transaction};


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
pub struct App {
    gh_creds: GithubOAuthCredentials,
    pkey: PKey<pkey::Private>,
    pubkey: PKey<pkey::Public>,
    dbenv: Arc<lmdb::Environment>,
    default_db: Arc<lmdb::Database>,
    user_db: Arc<lmdb::Database>,
    base_url: String,
}

fn legal_path_for_user(key: &str, login: &String) -> bool {
    let regexps = vec![
        format!("cos316/assignments"),
        format!("cos316/assignments/[^/]/{}", login),
    ];
    for re in regexps.iter().map(|re| regex::Regex::new(&re.as_str())).filter_map(Result::ok) {
        if re.is_match(key) {
            return true;
        }
    }
    false
}

impl App {
    pub fn new(gh_creds: GithubOAuthCredentials, pkey: PKey<pkey::Private>, pubkey: PKey<pkey::Public>, dbenv: lmdb::Environment, base_url: String) -> App {
        let dbenv = Arc::new(dbenv);
        let default_db = Arc::new(dbenv.open_db(None).unwrap());
        let user_db = Arc::new(dbenv.create_db(Some("users"), lmdb::DatabaseFlags::empty()).unwrap());
        App {
            dbenv,
            default_db,
            user_db,
            pkey, 
            pubkey,
            gh_creds,
            base_url,
        }
    }

    fn verify_jwt(&self, request: &Request) -> Result<String, Response> {
        let jwt = request.header("Authorization").and_then(|header| header.split(" ").last()).ok_or(Response::empty_400())?;
        let key = PKeyWithDigest {
            key: self.pubkey.clone(),
            digest: openssl::hash::MessageDigest::sha256(),
        };
        let claims: Claims = jwt.verify_with_key(&key).map_err(|e| {
            e
        }).map_err(|_| Response::empty_400())?;
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        if claims.exp < now && false { // TODO: remove false for production
            Err(Response::json(&serde_json::json!({
                "error": "Authentication token expired"
            })).with_status_code(403))
        } else {
            Ok(claims.sub)
        }
    }
}

impl App {
    pub fn handle(&mut self, request: &Request) -> Response {
        println!("{:?}", request);
        if request.method().to_uppercase().as_str() == "OPTIONS" {
            return Response::empty_204()
                .with_additional_header("Access-Control-Allow-Origin", "*")
                .with_additional_header("Access-Control-Allow-Headers", "Authorization")
                .with_additional_header("Access-Control-Allow-Methods", "*");
            
        }
        rouille::router!(request,
            (GET) (/login/github) => {
                Ok(Response::redirect_302(
                    format!("https://github.com/login/oauth/authorize?client_id={}&scopes=repo:invites", self.gh_creds.client_id)))
            },
            (GET) (/authenticate/github) => {
                self.auth_github(request)
            },
            (POST) (/pair_github) => {
                self.pair_github_to_user(request)
            },
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
            (GET) (/get) => {
                self.get(request)
            },
            (POST) (/put) => {
                self.put(request)
            },
            (GET) (/assignments) => {
                self.assignments(request)
            },
            _ => Ok(Response::empty_404())
        ).unwrap_or_else(|e| e).with_additional_header("Access-Control-Allow-Origin", "*")
    }

    fn whoami(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;
        #[derive(Serialize)]
        struct User {
            login: String,
            github: Option<String>,
        }
        let txn = self.dbenv.begin_ro_txn().unwrap();
        let github: Option<String> = txn.get(*self.user_db, &format!("github/for/user/{}", login).as_bytes()).ok().map(|l| String::from_utf8_lossy(l).to_string());
        Ok(Response::json(&User { login, github }))
    }

    fn assignments(&self, request: &Request) -> Result<Response, Response> {
        self.verify_jwt(request)?;

        let txn = self.dbenv.begin_ro_txn().unwrap();
        let results = txn.get(*self.default_db, &"cos316/assignments").ok()
                .map(String::from_utf8_lossy);
        let res = Response::json(&results);
        txn.commit().expect("commit");
        Ok(res)
    }

    fn get(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;

        let keys = request.get_param("keys").unwrap_or(String::new());
        let txn = self.dbenv.begin_ro_txn().unwrap();
        let admins: Vec<String> = txn.get(*self.user_db, &"admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
        let val = {
            let mut results = BTreeMap::new();
            for ref key in keys.split(",") {
                if admins.contains(&login) || legal_path_for_user(key, &login) {
                    results.insert(*key,
                        txn.get(*self.default_db, &key).ok()
                            .map(String::from_utf8_lossy)
                    );
                }
            }
            results
        };
        let res = Response::json(&val);
        txn.commit().expect("commit");
        Ok(res)
    }

    fn put(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;

        let mut input = rouille::input::multipart::get_multipart_input(request).or(Err(Response::empty_400()))?;

        let mut txn = self.dbenv.begin_rw_txn().unwrap();
        let admins: Vec<String> = txn.get(*self.user_db, &"admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
        let res = if admins.iter().find(|l| **l == login).is_some() {
            while let Some(mut field) = input.next() {
                let mut data = Vec::new();
                field.data.read_to_end(&mut data).expect("read");
                txn.put(*self.default_db, &field.headers.name.as_bytes(), &data.as_slice(), lmdb::WriteFlags::empty()).expect("store data");
            }
            Response::empty_204()
        } else {
            Response::empty_400()
        };
        txn.commit().expect("commit");
        Ok(res)
    }

    fn authenticate_cas(&self, request: &Request) -> Result<Response, Response> {
        let ticket = request.get_param("ticket").ok_or(Response::empty_404())?;
        let service = format!("{}/authenticate/cas", self.base_url);

        let client = Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
        let validate_cas = client
            .get(format!("{}/validate", "https://fed.princeton.edu/cas"))
            .query(&[("ticket", ticket), ("service", service)])
            .send().expect("reqwest");
        let sub = validate_cas.text().or(Err(Response::empty_400())).and_then(|text| {
            let result: Vec<&str> = text.lines().collect();
            match result.as_slice() {
                ["yes", user] => {
                    Ok(format!("{}@princeton.edu", user))
                },
                _ => Err(Response::empty_400()),
            }
        })?;

        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();

        let claims = Claims {
            alg: "ES256".to_string(),
            iat: now,
            exp: now + 10 * 60,
            sub,
        };
        let key = PKeyWithDigest {
            key: self.pkey.clone(),
            digest: openssl::hash::MessageDigest::sha256(),
        };
        let token = claims.sign_with_key(&key).unwrap();
        Ok(Response::html(format!(include_str!("authenticated_cas.html"), token)))
    }

    fn auth_github(&self, request: &Request) -> Result<Response, Response> {
        let code = request.get_param("code").ok_or(Response::empty_404())?;
        let client = Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
        let uat = client
            .post(format!("https://github.com/login/oauth/access_token"))
            .header(reqwest::header::ACCEPT, "application/vnd.github.v3+json")
            .header(reqwest::header::USER_AGENT, "SnapFaaS Web Frontend")
            .multipart(reqwest::blocking::multipart::Form::new()
                .text("client_id", self.gh_creds.client_id.clone())
                .text("client_secret", self.gh_creds.client_secret.clone())
                .text("code", code)
            )
            .send().expect("reqwest");

        #[derive(Debug, Deserialize)]
        struct AuthResponse {
            access_token: String,
        }
        let t: AuthResponse = uat.json().map_err(|_| Response::empty_400())?;
        Ok(Response::html(format!(include_str!("authenticated_github.html"), token=t.access_token, base_url=self.base_url)))
    }

    fn pair_github_to_user(&self, request: &Request) -> Result<Response, Response> {
        let local_user = self.verify_jwt(request)?;

        let input = rouille::post_input!(request, {
            github_token: String,
        }).map_err(|e| { println!("{:?}",e); Response::empty_400() })?;
        let client = Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
        let github_user: github_types::User = client
            .get(format!("https://api.github.com/user"))
            .header(reqwest::header::ACCEPT, "application/vnd.github.v3+json")
            .header(reqwest::header::USER_AGENT, "SnapFaaS Web Frontend")
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", input.github_token))
            .send().expect("reqwest").json().unwrap();
        let mut txn = self.dbenv.begin_rw_txn().unwrap();
        let user_db = *self.user_db;
        txn.put(user_db, &format!("github/for/user/{}", &local_user).as_str(), &github_user.login.as_str(), lmdb::WriteFlags::empty()).expect("store user");
        txn.put(user_db, &format!("github/user/{}/token", &github_user.login).as_str(), &input.github_token.as_str(), lmdb::WriteFlags::empty()).expect("store user");
        txn.put(user_db, &format!("github/from/{}", &github_user.login).as_str(), &local_user.as_str(), lmdb::WriteFlags::empty()).expect("store user");
        txn.commit().expect("commit");
        Ok(Response::json(&github_user.login))
    }
}
