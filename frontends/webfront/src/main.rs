use clap::{App, Arg};
use openssl::pkey::PKey;
use snapfaas::{blobstore::Blobstore, labeled_fs};

mod app;
pub mod init;

fn main() -> Result<(), std::io::Error> {
    dotenv::dotenv().ok();
    env_logger::init();

    let github_client_id = std::env::var("GITHUB_CLIENT_ID").expect("client id");
    let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET").expect("client secret");

    let matches = App::new("SnapFaaS API Web Server")
        .arg(
            Arg::with_name("storage path")
                .short('s')
                .long("storage")
                .value_name("PATH")
                .takes_value(true)
                .required(false)
                .default_value("storage")
                .help("Path to LMDB storage"),
        )
        .arg(
            Arg::with_name("blob path")
                .long("blobs")
                .value_name("PATH")
                .takes_value(true)
                .required(false)
                .default_value("blobs")
                .help("Path to blob storage"),
        )
        .arg(
            Arg::with_name("tmp path")
                .long("tmp")
                .value_name("PATH")
                .takes_value(true)
                .required(false)
                .default_value("tmp")
                .help("Path to temporary blob storage"),
        )
        .arg(
            Arg::with_name("listen")
                .long("listen")
                .short('l')
                .takes_value(true)
                .value_name("ADDR:PORT")
                .required(true)
                .help("Address to listen on"),
        )
        .arg(
            Arg::with_name("secret key")
                .long("secret_key")
                .short('k')
                .takes_value(true)
                .value_name("PATH")
                .required(true)
                .help("PEM encoded private key"),
        )
        .arg(
            Arg::with_name("public key")
                .long("public_key")
                .short('p')
                .takes_value(true)
                .value_name("PATH")
                .required(true)
                .help("PEM encoded public key"),
        )
        .arg(
            Arg::with_name("base url")
                .long("base_url")
                .takes_value(true)
                .value_name("URL")
                .required(true)
                .help("Base URL of server"),
        )
        .arg(
            Arg::with_name("faasten scheduler address")
                .long("faasten_scheduler")
                .value_name("[ADDR:]PORT")
                .takes_value(true)
                .required(true)
                .help("Address of the Faasten scheduler"),
        )
        .get_matches();

    //let dbenv = lmdb::Environment::new()
    //    .set_map_size(100 * 1024 * 1024 * 1024)
    //    .set_max_dbs(2)
    //    .open(&std::path::Path::new(
    //        matches.value_of("storage path").unwrap(),
    //    ))
    //    .unwrap();
    let public_key_bytes = std::fs::read(matches.value_of("public key").expect("public key"))?;
    let private_key_bytes = std::fs::read(matches.value_of("secret key").expect("private key"))?;
    let base_url = matches.value_of("base url").expect("base url").to_string();
    let sched_address = matches
        .value_of("faasten scheduler address")
        .unwrap()
        .to_string();
    let blobstore = Blobstore::new(
        matches.value_of("blob path").unwrap().into(),
        matches.value_of("tmp path").unwrap().into(),
    );
    let fs = snapfaas::fs::FS::new(&*labeled_fs::DBENV);
    let app = app::App::new(
        app::GithubOAuthCredentials {
            client_id: github_client_id,
            client_secret: github_client_secret,
        },
        PKey::private_key_from_pem(private_key_bytes.as_slice()).unwrap(),
        PKey::public_key_from_pem(public_key_bytes.as_slice()).unwrap(),
        //dbenv,
        blobstore,
        fs,
        base_url,
        sched_address,
    );
    let listen_addr = matches.value_of("listen").unwrap();
    rouille::start_server(listen_addr, move |request| {
        use log::{error, info};
        use rouille::{Request, Response};

        let mut app = app.clone();

        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.6f");
        let log_ok = |req: &Request, resp: &Response, _elap: std::time::Duration| {
            info!(
                "{} {} {} - {}",
                now,
                req.method(),
                req.raw_url(),
                resp.status_code
            );
        };
        let log_err = |req: &Request, _elap: std::time::Duration| {
            error!(
                "{} Handler panicked: {} {}",
                now,
                req.method(),
                req.raw_url()
            );
        };
        rouille::log_custom(request, log_ok, log_err, || app.handle(request))
    });
}
