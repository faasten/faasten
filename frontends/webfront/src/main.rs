use clap::{App, Arg};
use openssl::pkey::PKey;

mod app;

fn main() -> Result<(), std::io::Error> {
    dotenv::dotenv().ok();

    let github_client_id = std::env::var("GITHUB_CLIENT_ID").expect("client id");
    let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET").expect("client secret");

    let matches = App::new("SnapFaaS API Web Server")
        .arg(
            Arg::with_name("storage path")
                .short("s")
                .long("storage")
                .value_name("PATH")
                .takes_value(true)
                .required(false)
                .default_value("storage")
                .help("Path to LMDB storage"),
        )
        .arg(
            Arg::with_name("listen")
                .long("listen")
                .short("l")
                .takes_value(true)
                .value_name("ADDR:PORT")
                .required(true)
                .help("Address to listen on"),
        )
        .arg(
            Arg::with_name("secret key")
                .long("secret_key")
                .short("k")
                .takes_value(true)
                .value_name("PATH")
                .required(true)
                .help("PEM encoded private key"),
        )
        .arg(
            Arg::with_name("public key")
                .long("public_key")
                .short("p")
                .takes_value(true)
                .value_name("PATH")
                .required(true)
                .help("PEM encoded public key"),
        )
        .get_matches();


    let dbenv = lmdb::Environment::new()
        .set_map_size(4096 * 1024 * 1024)
        .set_max_dbs(2)
        .open(&std::path::Path::new(matches.value_of("storage path").unwrap()))
        .unwrap();
    let app = app::App::new(
        app::GithubOAuthCredentials {
            client_id: github_client_id,
            client_secret: github_client_secret,
        },
        PKey::private_key_from_pem(include_bytes!("../secret.pem")).unwrap(),
        PKey::public_key_from_pem(include_bytes!("../public.pem")).unwrap(),
        dbenv
    );
    let listen_addr = matches.value_of("listen").unwrap();
    rouille::start_server(listen_addr, move |request| {
        let mut app = app.clone();
        app.handle(request)
    });
}
