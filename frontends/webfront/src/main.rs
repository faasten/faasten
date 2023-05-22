use clap::Parser;
use openssl::pkey::PKey;
use snapfaas::{blobstore::Blobstore, cli, fs::BackingStore};

mod app;
pub mod init;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    store: cli::Store,
    /// Path of the blob directory
    #[arg(long, value_name = "PATH", default_value = "blobs")]
    blobs: std::ffi::OsString,
    /// PATH of the tmpfs directory path
    #[arg(long, value_name = "PATH", default_value = "tmp")]
    tmp: std::ffi::OsString,
    /// Address to listen on
    #[arg(short, long, value_name = "ADDR:PORT")]
    listen: String,
    /// Path of the PEM encoded secret key
    #[arg(short = 'k', long, value_name = "PATH")]
    secret_key: std::ffi::OsString,
    /// Path of the PEM encoded public key
    #[arg(short = 'p', long, value_name = "PATH")]
    public_key: std::ffi::OsString,
    /// Base URL of the gateway server
    #[arg(long, value_name = "URL")]
    base_url: String,
    /// Address of the Faasten scheduler
    #[arg(long, value_name = "ADDR:PORT")]
    faasten_scheduler: String,
}

fn main() -> Result<(), std::io::Error> {
    env_logger::init();

    let cli = Cli::parse();

    let public_key_bytes = std::fs::read(cli.public_key)?;
    let private_key_bytes = std::fs::read(cli.secret_key)?;
    let base_url = cli.base_url;
    let sched_address = cli.faasten_scheduler;
    let blobstore = Blobstore::new(cli.blobs, cli.tmp);
    let listen_addr = cli.listen;
    if let Some(tikv_pds) = cli.store.tikv {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let client =
            rt.block_on(async { tikv_client::RawClient::new(tikv_pds, None).await.unwrap() });
        let tikv = snapfaas::fs::tikv::TikvClient::new(client, std::sync::Arc::new(rt));
        let app = app::App::new(
            PKey::private_key_from_pem(private_key_bytes.as_slice()).unwrap(),
            PKey::public_key_from_pem(public_key_bytes.as_slice()).unwrap(),
            blobstore,
            tikv,
            base_url,
            sched_address,
        );
        start_app(app, &listen_addr)
    } else if let Some(path) = cli.store.lmdb {
        let dbenv = std::boxed::Box::leak(Box::new(
            lmdb::Environment::new()
                .set_map_size(100 * 1024 * 1024 * 1024)
                .set_max_dbs(2)
                .open(&std::path::Path::new(&path))
                .unwrap(),
        ));
        let app = app::App::new(
            PKey::private_key_from_pem(private_key_bytes.as_slice()).unwrap(),
            PKey::public_key_from_pem(public_key_bytes.as_slice()).unwrap(),
            blobstore,
            &*dbenv,
            base_url,
            sched_address,
        );
        start_app(app, &listen_addr)
    } else {
        panic!("We shouldn't reach here.")
    }
}

fn start_app<B>(app: app::App<B>, listen_addr: &str) -> Result<(), std::io::Error>
where
    B: BackingStore + Clone + Send + 'static + Sync,
{
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
