#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, Arg};
use lmdb::{Transaction, WriteFlags};
use byteorder::{BigEndian, ByteOrder};
use std::io::Write;

fn main() {
    let cmd_arguments = App::new("fireruner wrapper")
        .version(crate_version!())
        .author(crate_authors!())
        .about("launch a single firerunner vm.")
        .arg(
            Arg::with_name("STORAGE")
                .short("s")
                .long("storage")
                .required(false)
                .default_value("storage")
                .takes_value(true)
                .value_name("DIR")
            )
        .arg(
            Arg::with_name("INT")
                .short("i")
                .required(false)
                .help("store/parse value as 32-bit integer")
            )
        .arg(
            Arg::with_name("BINARY")
                .short("b")
                .required(false)
                .help("output value as raw binary")
            )
        .arg(
            Arg::with_name("KEY")
                .required(true)
                .index(1)
            )
        .arg(
            Arg::with_name("VALUE")
                .required(false)
                .index(2)
        ).get_matches();

    let dbenv = lmdb::Environment::new()
        .open(std::path::Path::new(cmd_arguments.value_of("STORAGE").unwrap()))
        .unwrap();
    let default_db = dbenv.open_db(None).unwrap();

    let mut buf = [0; 4];
    let key = cmd_arguments.value_of("KEY").unwrap();
    if let Some(value) = cmd_arguments.value_of("VALUE") {
        let mut txn = dbenv.begin_rw_txn().unwrap();
        if value.is_empty() {
            println!("{}", txn.del(default_db, &key, None).is_ok());
        } else if value == "-" {
            let mut value_bytes = Vec::new();
            let _ = std::io::Read::read_to_end(&mut std::io::stdin(), &mut value_bytes);
            println!("{}", txn.put(default_db, &key, &value_bytes, WriteFlags::empty()).is_ok());
        } else {
            let value = if cmd_arguments.is_present("INT") {
                BigEndian::write_u32(&mut buf, value.parse().expect("parse u32"));
                &buf[..]
            } else {
                value.as_ref()
            };
            println!("{}", txn.put(default_db, &key, &value, WriteFlags::empty()).is_ok());
        }
        let _ = txn.commit();
    } else {
        let txn = dbenv.begin_ro_txn().unwrap();
        if let Ok(value) = txn.get(default_db, &key) {
            if cmd_arguments.is_present("INT") {
            println!("{}", BigEndian::read_u32(value));
            } else if cmd_arguments.is_present("BINARY") {
               let _ = std::io::stdout().write_all(value);
            } else {
                println!("{}", String::from_utf8_lossy(value));
            }
        } else {
            println!("NOT FOUND");
        }
        let _ = txn.commit();

    }
}
