use clap::Parser;
use labeled::buckle::Buckle;
use snapfaas::{cli, fs};
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[clap(author, version, about, long_about=None)]
struct Cli {
    /// Periodically run garbage collection
    #[arg(short, long, value_name = "SECS", default_value_t = 60)]
    interval: u64,
    /// Run garbage collection once
    #[arg(long, conflicts_with = "interval")]
    once: bool,
    #[command(flatten)]
    store: cli::Store,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let interval = cli.interval;

    if let Some(_) = cli.store.tikv {
        todo!();
    } else if let Some(lmdb) = cli.store.lmdb.as_ref() {
        fs::utils::taint_with_label(Buckle::top());
        let dbenv = std::boxed::Box::leak(Box::new(snapfaas::fs::lmdb::get_dbenv(lmdb)));
        let mut fs = fs::FS::new(&*dbenv);
        loop {
            if let Ok(collected) = fs.collect_garbage() {
                log::debug!("garbage collected {}", collected.len())
            }
            if cli.once {
                break;
            } else {
                thread::sleep(Duration::new(interval, 0));
            }
        }
    }
}
