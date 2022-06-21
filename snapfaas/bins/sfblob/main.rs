#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, Arg};
use sha2::Sha256;
use snapfaas::blobstore::Blobstore;
use std::{io::{stdin, copy, BufRead, stdout}, path::Path, ffi::OsString};

fn main() -> std::io::Result<()> {
    let cmd_arguments = App::new("SnapFaaS Blob CLI")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Read and write blobs")
        .arg(
            Arg::with_name("READ")
                .short("r")
                .required(false)
                .help("read names from stdout")
            )
        .arg(
            Arg::with_name("STORAGE")
                .short("s")
                .long("storage")
                .required(false)
                .default_value("blobs")
                .takes_value(true)
                .value_name("DIR")
            ).get_matches();

    let base_dir_path = Path::new(cmd_arguments.value_of("STORAGE").unwrap());
    let tmp_dir_path = base_dir_path.join("tmp");
    let _ = std::fs::create_dir_all(&tmp_dir_path);
    let base_dir = OsString::from(base_dir_path);
    let mut blobstore = Blobstore::<Sha256>::new(base_dir, OsString::from(tmp_dir_path));

    let mut stdin = stdin();
    if cmd_arguments.is_present("READ") {
        for line in stdin.lock().lines() {
            let mut blob = blobstore.open(line?)?;
            copy(&mut blob, &mut stdout())?;
        }
    } else {
        let mut newblob = blobstore.create()?;
        copy(&mut stdin, &mut newblob)?;
        println!("{}", blobstore.save(newblob)?.name);
    }
    Ok(())
}
