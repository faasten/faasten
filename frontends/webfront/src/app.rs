use std::collections::BTreeMap;
use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;
use std::io::Read;
use std::net::TcpStream;

use labeled::buckle;
use reqwest::blocking::Client;
use rouille::post_input;
use serde::{Deserialize, Serialize};
use rouille::{Request, Response, ResponseBody, input::post::BufferedFile};
use jwt::{PKeyWithDigest, SignWithKey, VerifyWithKey};
use openssl::pkey::{self, PKey};
use lmdb::Transaction;
use log::error;

use snapfaas::blobstore::Blobstore;
use snapfaas::fs;
use snapfaas::fs::FS;
use snapfaas::request;
use snapfaas::sched;
use snapfaas::syscall_server::buckle_to_pblabel;
use snapfaas::syscall_server::component_to_pbcomponent;
use snapfaas::syscalls;

struct FaastenScheduler {
    sched_address: String,
}

impl r2d2::ManageConnection for FaastenScheduler {
    type Connection = TcpStream;
    type Error = std::io::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(TcpStream::connect(&self.sched_address)?)
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        use std::io::{ErrorKind, Error};
        sched::rpc::ping(conn).map_err(|e| Error::new(ErrorKind::Other, format!("{:?}", e)))?;
        Ok(())
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.take_error().ok().flatten().is_some()
    }
}

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
    blobstore: Arc<Mutex<Blobstore>>,
    base_url: String,
    conn: r2d2::Pool<FaastenScheduler>,
}

impl App {
    pub fn new(gh_creds: GithubOAuthCredentials, pkey: PKey<pkey::Private>, pubkey: PKey<pkey::Public>, dbenv: lmdb::Environment, blobstore: Blobstore, base_url: String, sched_address: String) -> App {
        let dbenv = Arc::new(dbenv);
        let default_db = Arc::new(dbenv.open_db(None).unwrap());
        let conn = r2d2::Pool::builder().max_size(10).build(FaastenScheduler { sched_address }).expect("pool");
        let blobstore = Arc::new(Mutex::new(blobstore));
        App {
            conn,
            dbenv,
            default_db,
            blobstore,
            pkey,
            pubkey,
            gh_creds,
            base_url,
        }
    }

    fn legal_path_for_user<T: Transaction>(&self, key: &str, login: &String, txn: &T) -> bool {
        let admins: Vec<String> = txn.get(*self.default_db, &"users/admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
        if admins.contains(login) {
            return true;
        }
        let regexps = regex::RegexSet::new(&[
            format!("^[^/]+/enrollments.json$"),
            format!("^[^/]+/assignments$"),
            format!("^[^/]+/assignments/[^/]+/{}$", login),
        ]).unwrap();
        if regexps.is_match(key) {
            return true;
        }
        if let Ok(Some(captures)) = regex::Regex::new("github/([^/]/[^/])/?.*").map(|r| r.captures(key)) {
            let mut s = String::new();
            captures.expand("$1/_meta", &mut s);
            #[derive(Deserialize)]
            struct Meta {
                users: Vec<String>
            }
            if let Some(meta) = txn.get(*self.default_db, &s.as_bytes()).map(|b| serde_json::from_slice::<Meta>(b).ok()).ok().flatten() {
                return meta.users.contains(login);
            }
        }
        false
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

    pub fn handle(&mut self, request: &Request) -> Response {
        if request.method().to_uppercase().as_str() == "OPTIONS" {
            return Response::empty_204()
                .with_additional_header("Access-Control-Allow-Origin", "*")
                .with_additional_header("Access-Control-Allow-Headers", "Authorization, Content-type")
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
            (GET) (/get_blob) => {
                self.get_blob(request)
            },
            (GET) (/read_dir) => {
                self.read_dir(request)
            },
            (POST) (/put) => {
                self.put(request)
            },
            (DELETE) (/delete) => {
                self.delete(request)
            },
            (POST) (/put_blob) => {
                self.put_blob(request)
            },
            (GET) (/assignments) => {
                self.assignments(request)
            },
            (POST) (/assignments) => {
                self.start_assignment(request)
            },
            (POST) (/invoke/{function_name}) => {
                self.invoke(request, function_name)
            },
            (GET) (/faasten/ping) => {
                Ok(Response::text("Pong.").with_status_code(200))
            },
            (POST) (/faasten/invoke/{gate_path}) => {
                self.faasten_init(request, gate_path)
            },
            _ => {
                error!("404: {} {}", request.method(), request.raw_url());
                Ok(Response::empty_404())
            }
        ).unwrap_or_else(|e| e).with_additional_header("Access-Control-Allow-Origin", "*")
    }

    // "init" code for any function invocation. It is responsible for resolving the passed-in
    // gate path in a secure way. That is, it makes sure to set the privilege and the clearance
    // for a logged in user or a public user. Moreover, it tracks the label for the file system
    // traversal and set LabeledInvoke.label to the new label after the traversal.
    fn faasten_init(&self, request: &Request, gate_path: String) -> Result<Response, Response> {
        let login = self.verify_jwt(request).ok();

        let conn = &mut self.conn.get().map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to get scheduler connection"
            })).with_status_code(500))?;

        // process the input:
        // 1. generate labeled_invoke from the input
        // 2. if file presents, store it to the blobstore.
        let input_post_form = post_input!(request, {
            file: Option<BufferedFile>, payload: String, label: String
        }).map_err(|e|
            Response::json(&serde_json::json!({ "error": e.to_string() })).with_status_code(400)
        )?;

        // parse the label
        let label = buckle::Buckle::parse(&input_post_form.label)
            .map_err(|e| Response::json(&serde_json::json!({ "error": e.to_string() }))
                .with_status_code(400))?;

        // parse the gate path
        use syscalls::{PathComponent, path_component::Component};
        let mut sc_path = Vec::new();
        for c in gate_path.split(":") {
            if c.starts_with("^") {
                let facet = buckle::Buckle::parse(c.strip_prefix("^").unwrap())
                    .map_err(|e| Response::json(&serde_json::json!({ "error": e.to_string() }))
                        .with_status_code(400))?;
                let facet = buckle_to_pblabel(&facet);
                sc_path.push(PathComponent{ component: Some(Component::Facet(facet)) });
            } else {
                sc_path.push(PathComponent{ component: Some(Component::Dscrp(c.to_string())) });
            }
        }

        // resolve the gate path
        // FIXME this storage setup requires this gateway and worker machines to have access to
        // the same database file.
        let faasten_fs = FS::new(&*self.dbenv);

        snapfaas::fs::utils::clear_label();
        snapfaas::fs::utils::taint_with_label(label);
        let (privilege, clearance) = match login {
            Some(login) =>
                (buckle::Buckle::parse(&("T,".to_string()+&login)).unwrap().integrity,
                 buckle::Buckle::parse(&(login+",T")).unwrap()),
            None =>
                (buckle::Buckle::parse("T,T").unwrap().integrity,
                 buckle::Buckle::public())
        };
        snapfaas::fs::utils::set_my_privilge(privilege);
        snapfaas::fs::utils::set_clearance(clearance);
        let (name, gate_privilege) = snapfaas::fs::utils::invoke_clearance_check(&faasten_fs, &sc_path).map_err(
            |e| Response::json(&serde_json::json!({"error": format!("{:?}", e)}))
            .with_status_code(400))?;
        let gate_privilege = component_to_pbcomponent(&gate_privilege);
        let label = fs::utils::get_current_label();
        let label = buckle_to_pblabel(&label);

        let val: serde_json::Value = serde_json::from_str(&input_post_form.payload).map_err(|e|
            Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(400))?;
        let mut payload = serde_json::json!({"input": val});
        // store file into the blobstore
        if let Some(f) = input_post_form.file {
            let mut blobstore = self.blobstore.lock().map_err(|e|
                Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
            )?;
            let mut newblob = blobstore.create().map_err(|e|
                Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
            )?;
            newblob.write_all(f.data.as_ref()).map_err(|e|
                Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
            )?;
            let name = blobstore.save(newblob).map_err(|e|
                Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
            )?.name;
            payload.as_object_mut().unwrap().insert("input-blob".to_string(), serde_json::Value::String(name));
        };

        // generate the labeled_invoke
        let req = sched::message::LabeledInvoke {
            name, label: Some(label), gate_privilege, payload: payload.to_string(), sync: true
        };

        // submit the labeled_invoke to the scheduler
        sched::rpc::labeled_invoke(conn, req).map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to submit invocation to the scheduler",
            })).with_status_code(500))?;

        // wait for the return
        let ret = sched::message::read_u8(conn).map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to read the task return",
            })).with_status_code(500))?;
        Ok(Response::from_data("application/octet-stream", ret))
    }

    fn whoami(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;
        #[derive(Serialize)]
        struct User {
            login: String,
            github: Option<String>,
        }
        let txn = self.dbenv.begin_ro_txn().unwrap();
        let github: Option<String> = txn.get(*self.default_db, &format!("users/github/for/user/{}", login).as_bytes()).ok().map(|l| String::from_utf8_lossy(l).to_string());
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

    fn invoke(&self, request: &Request, function_name: String) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;

        let conn = &mut self.conn.get().map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to get snapfaas connection"
            })).with_status_code(500))?;
        let input_json: serde_json::Value = rouille::input::json_input(request).map_err(|e| Response::json(&serde_json::json!({ "error": e.to_string() })).with_status_code(400))?;

        let txn = self.dbenv.begin_ro_txn().unwrap();
        let admins: Vec<String> = txn.get(*self.default_db, &"users/admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
        txn.commit().expect("commit");
        if !(admins.contains(&login) || ["start_assignment", "add_to_repo", "delete_repo", "cos326_find_ungraded", "cos326_find_not_submitted"].contains(&function_name.as_str())) {
            return Err(Response::json(&serde_json::json!({ "error": "user not authorized to make request" })).with_status_code(401))
        }

        let req = request::Request {
            gate: function_name,
            payload: serde_json::json!({
                "payload": input_json,
                "login": login,
            }),
        };
        request::write_u8(&req.to_vec(), conn).map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to send request"
            })).with_status_code(500))?;

        let resp_buf = request::read_u8(conn).map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to read response"
            })).with_status_code(500))?;
        let rsp: request::Response = serde_json::from_slice(&resp_buf).unwrap();
        match rsp.status {
            request::RequestStatus::SentToVM(response) => Ok(Response::text(response)),
            _ => Err(Response::json(&serde_json::json!({"error": format!("{:?}", rsp.status)}))),
        }
    }

    fn start_assignment(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;

        let conn = &mut self.conn.get().map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to get snapfaas connection"
            })).with_status_code(500))?;
        #[derive(Debug, Serialize, Deserialize)]
        struct Input {
            assignment: String,
            users: Vec<String>,
            course: String,
        }
        let input_json: Input = rouille::input::json_input(request).map_err(|e| Response::json(&serde_json::json!({ "error": e.to_string() })).with_status_code(400))?;

        let txn = self.dbenv.begin_ro_txn().unwrap();
        let admins: Vec<String> = txn.get(*self.default_db, &"users/admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
        if !(input_json.users.contains(&login) || admins.contains(&login)) {
            return Err(Response::json(&serde_json::json!({ "error": "user not authorized to make request" })).with_status_code(401))
        }

        let mut gh_handles = vec![];
        for user in input_json.users.iter() {
            let gh_handle = txn.get(*self.default_db, &format!("users/github/for/user/{}", user).as_str()).or(
                Err(
                    Response::json(&serde_json::json!({ "error": format!("no github handle for \"{}\"", user) })).with_status_code(400)
                )
            )?;
            gh_handles.push(String::from_utf8_lossy(gh_handle).to_string());
        }
        txn.commit().expect("commit");

        let req = request::Request {
            gate: "start_assignment".to_string(),
            payload: serde_json::json!({
                "assignment": input_json.assignment,
                "users": input_json.users,
                "course": input_json.course,
                "gh_handles": gh_handles,

                "payload": input_json,
                "login": login
            }),
        };
        request::write_u8(&req.to_vec(), conn).map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to send request"
            })).with_status_code(500))?;

        let resp_buf = request::read_u8(conn).map_err(|_|
            Response::json(&serde_json::json!({
                "error": "failed to read response"
            })).with_status_code(500))?;
        let rsp: request::Response = serde_json::from_slice(&resp_buf).unwrap();
        match rsp.status {
            request::RequestStatus::SentToVM(response) => Ok(Response::text(response)),
            _ => Err(Response::json(&serde_json::json!({"error": format!("{:?}", rsp.status)}))),
        }
    }

    fn get(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;

        let keys = request.get_param("keys").unwrap_or(String::new());
        let txn = self.dbenv.begin_ro_txn().unwrap();
        let val = {
            let mut results = BTreeMap::new();
            for ref key in keys.split(",") {
                if self.legal_path_for_user(key, &login, &txn) {
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

    fn read_dir(&self, request: &Request) -> Result<Response, Response> {
        use lmdb::Cursor;

        let login = self.verify_jwt(request)?;

        let mut keys: HashSet<String> = HashSet::new();
        let mut dir = request.get_param("dir").ok_or(Response::empty_404())?;
        if !dir.ends_with('/') {
           dir = dir + "/";
        }

        let txn = self.dbenv.begin_ro_txn().unwrap();
        if self.legal_path_for_user(dir.as_str(), &login, &txn) {
            let mut cursor = txn.open_ro_cursor(*self.default_db).or(Err(Response::empty_400()))?.iter_from(&dir);
            while let Some(Ok((key, _))) = cursor.next() {
                if !key.starts_with(dir.as_bytes()) {
                    break
                }
                if let Some(entry) = key.split_at(dir.len()).1.split_inclusive(|c| *c == b'/').next() {
                    if !entry.is_empty() && self.legal_path_for_user(&String::from_utf8_lossy(key).to_owned(), &login, &txn) {
                        keys.insert(String::from_utf8_lossy(entry).into());
                    }
                }
            }
        }
        let _ = txn.commit();

        Ok(Response::json(&keys))
    }

    fn put(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;

        let mut input = rouille::input::multipart::get_multipart_input(request).or(Err(Response::empty_400()))?;

        let mut txn = self.dbenv.begin_rw_txn().unwrap();
        let admins: Vec<String> = txn.get(*self.default_db, &"users/admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
        let res = if admins.iter().find(|l| **l == login).is_some() {
            while let Some(mut field) = input.next() {
                let mut data = Vec::new();
                field.data.read_to_end(&mut data).expect("read");
                txn.put(*self.default_db, &field.headers.name.as_bytes(), &data.as_slice(), lmdb::WriteFlags::empty()).expect("store data");
            }
            txn.commit().expect("commit");
            Response::empty_204()
        } else {
            txn.abort();
            Response::empty_400()
        };
        Ok(res)
    }

    fn delete(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;

        let res = if let Some(key) = request.get_param("key") {
            let mut txn = self.dbenv.begin_rw_txn().unwrap();
            let admins: Vec<String> = txn.get(*self.default_db, &"users/admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
            if admins.iter().find(|l| **l == login).is_some() {
                let dres = txn.del(*self.default_db, &key, None);
                txn.commit().expect("commit");
                match dres {
                    Ok(()) => Response::empty_204(),
                    Err(lmdb::Error::NotFound) => Response::empty_404(),
                    _ => panic!("delete {:?}", dres),
                }
            } else {
                txn.abort();
                Response::empty_400()
            }
        } else {
            Response::empty_404()
        };
        Ok(res)
    }

    fn put_blob(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;
        if let Some(key) = request.get_param("key") {

            let mut txn = self.dbenv.begin_rw_txn().unwrap();
            let admins: Vec<String> = txn.get(*self.default_db, &"users/admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
            if admins.iter().find(|l| **l == login).is_some() {
                let mut input = request.data().ok_or(Response::empty_406())?;
                let mut blob = self.blobstore.lock().expect("lock").create().or(Err(Response::empty_406()))?;
                std::io::copy(&mut input, &mut blob).or(Err(Response::empty_406()))?;
                blob.flush().or(Err(Response::empty_406()))?;
                let blob = self.blobstore.lock().expect("lock").save(blob).or(Err(Response::empty_406()))?;
                txn.put(*self.default_db, &key.as_bytes(), &blob.name.as_bytes(), lmdb::WriteFlags::empty()).expect("store data");
                txn.commit().expect("commit");
                Ok(Response::json(&blob.name))
            } else {
                txn.abort();
                Ok(Response::empty_400())
            }
        } else {
            Ok(Response::empty_400())
        }
    }

    fn get_blob(&self, request: &Request) -> Result<Response, Response> {
        let login = self.verify_jwt(request)?;
        if let Some(key) = request.get_param("key") {

            let txn = self.dbenv.begin_ro_txn().unwrap();
            let admins: Vec<String> = txn.get(*self.default_db, &"users/admins").ok().map(|x| serde_json::from_slice(x).ok()).flatten().unwrap_or(vec![]);
            if admins.iter().find(|l| **l == login).is_some() {
                if let Some(blob_key) = txn.get(*self.default_db, &key.as_bytes()).ok() {
                    let blob = self.blobstore
                                   .lock()
                                   .expect("lock")
                                   .open(String::from_utf8(blob_key.to_vec()).or(Err(Response::empty_406()))?)
                                   .or(Err(Response::empty_406()))?;
                    Ok(Response {
                        status_code: 200,
                        headers: vec![("Content-Type".into(), "application/octet-stream".into())],
                        data: ResponseBody::from_reader(blob),
                        upgrade: None,
                    })
                } else {
                    Ok(Response::empty_400())
                }
            } else {
                Ok(Response::empty_400())
            }
        } else {
            Ok(Response::empty_400())
        }
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
            sub: sub.clone(),
        };
        let key = PKeyWithDigest {
            key: self.pkey.clone(),
            digest: openssl::hash::MessageDigest::sha256(),
        };
        let token = claims.sign_with_key(&key).unwrap();

        // generate the per-user fsutil gate.
        let root_fsutil = vec![
            syscalls::PathComponent { component: Some(syscalls::path_component::Component::Dscrp("fsutil".to_string())) }
        ];
        let user_facet = buckle::Buckle::parse(&format!("{0},{0}", sub)).unwrap();
        let user_facet_pb = buckle_to_pblabel(&user_facet);
        let user_facet_dir_path = vec![
            syscalls::PathComponent { component: Some(syscalls::path_component::Component::Dscrp("home".to_string())) },
            syscalls::PathComponent { component: Some(syscalls::path_component::Component::Facet(user_facet_pb)) },
        ];
        let faasten_fs = FS::new(&*self.dbenv);
        fs::utils::dup_gate(&faasten_fs, &root_fsutil, &user_facet_dir_path, "fsutil".to_string(), user_facet)
            .map_err(|_| Response::text("failed to create the user-specific storage function"))?;

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
        txn.put(*self.default_db, &format!("users/github/for/user/{}", &local_user).as_str(), &github_user.login.as_str(), lmdb::WriteFlags::empty()).expect("store user");
        txn.put(*self.default_db, &format!("users/github/user/{}/token", &github_user.login).as_str(), &input.github_token.as_str(), lmdb::WriteFlags::empty()).expect("store user");
        txn.put(*self.default_db, &format!("users/github/from/{}", &github_user.login).as_str(), &local_user.as_str(), lmdb::WriteFlags::empty()).expect("store user");
        txn.commit().expect("commit");
        Ok(Response::json(&github_user.login))
    }
}
