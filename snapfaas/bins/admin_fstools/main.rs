//! Faasten file system preparer
//!
//! The preparer installs supported kernels and runtime images in the directory ``home:^T,faasten''.
//! Kernels and runtime images are stored as blobs.

use clap::{Parser, Subcommand};
use sha2::Sha256;
use snapfaas::{
    blobstore, cli,
    fs::{BackingStore, FS},
};
use std::io::{stdout, Write};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    action: Action,
    #[command(flatten)]
    store: cli::Store,
}

#[derive(Parser, Debug)]
struct Bootstrap {
    /// YAML configuration file for bootstraping
    #[arg(value_name = "YAML_PATH")]
    yaml: String,
}

#[derive(Parser, Debug)]
struct UpdateImage {
    /// Path of the new image
    #[arg(value_name = "LOCAL_PATH")]
    path: String,
}

#[derive(Parser, Debug)]
struct FaastenPath {
    /// Faasten path
    #[arg(value_name = "FAASTEN_PATH")]
    path: String,
}

#[derive(Parser, Debug)]
struct CreateBlob {
    /// Local path of the blob
    #[arg(value_name = "LOCAL_PATH")]
    src: String,
    /// Faasten path of the blob
    #[arg(value_name = "FAASTEN_PATH")]
    dest: String,
    /// Label of the blob in Faasten
    #[arg(value_name = "BUCKLE")]
    label: String,
}

#[derive(Parser, Debug)]
struct Mkdir {
    /// Faasten path of the new directory
    #[arg(value_name = "FAASTEN_PATH")]
    path: String,
    /// Label of the directory in Faasten
    #[arg(value_name = "BUCKLE")]
    label: String,
}

#[derive(Subcommand, Debug)]
enum Action {
    /// Bootstrap Faasten FS from the configuration file
    Bootstrap(Bootstrap),
    /// Update the fsutil image
    UpdateFsutil(UpdateImage),
    /// Update the python image
    UpdatePython(UpdateImage),
    /// List the Faasten directory
    List(FaastenPath),
    /// List the Faasten faceted directory
    FacetedList(FaastenPath),
    /// Read the Faasten file
    Read(FaastenPath),
    /// Delete the Faasten FS object
    Delete(FaastenPath),
    /// Create a blob from a local file
    CreateBlob(CreateBlob),
    /// Create a directory
    Mkdir(Mkdir),
}

pub fn main() -> std::io::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    let fs: FS<Box<dyn BackingStore>> = if let Some(tikv_pds) = cli.store.tikv {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let client =
            rt.block_on(async { tikv_client::RawClient::new(tikv_pds, None).await.unwrap() });
        FS::new(Box::new(snapfaas::fs::tikv::TikvClient::new(
            client,
            std::sync::Arc::new(rt),
        )))
    } else if let Some(lmdb) = cli.store.lmdb.as_ref() {
        let dbenv = std::boxed::Box::leak(Box::new(snapfaas::fs::lmdb::get_dbenv(lmdb)));
        FS::new(Box::new(&*dbenv))
    } else {
        panic!("We shouldn't reach here.")
    };

    let blobstore = blobstore::Blobstore::default();
    match cli.action {
        Action::Bootstrap(bs) => {
            snapfaas::fs::bootstrap::prepare_fs(&fs, &bs.yaml);
        }
        Action::UpdatePython(ui) => {
            snapfaas::fs::bootstrap::update_python(&fs, blobstore, &ui.path);
        }
        Action::UpdateFsutil(ui) => {
            snapfaas::fs::bootstrap::update_fsutil(&fs, blobstore, &ui.path);
        }
        Action::List(fp) => {
            let clearance = labeled::buckle::Buckle::parse("faasten,T").unwrap();
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());
            snapfaas::fs::utils::set_clearance(clearance);

            let path = snapfaas::fs::path::Path::parse(&fp.path).unwrap();
            match snapfaas::fs::utils::list(&fs, path) {
                Ok(entries) => {
                    for (name, dent) in entries {
                        println!("{}\t{:?}", name, dent);
                    }
                }
                Err(e) => log::warn!("Failed list. {:?}", e),
            }
        }
        Action::FacetedList(fp) => {
            let clearance = labeled::buckle::Buckle::parse("faasten,T").unwrap();
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());
            snapfaas::fs::utils::taint_with_label(clearance.clone());
            snapfaas::fs::utils::set_clearance(clearance);

            let path = snapfaas::fs::path::Path::parse(&fp.path).unwrap();
            match snapfaas::fs::utils::faceted_list(&fs, path) {
                Ok(entries) => {
                    for (f, entries) in entries {
                        println!("{}", f);
                        for entry in entries {
                            println!("\t{:?}", entry);
                        }
                    }
                }
                Err(e) => log::warn!("Failed list. {:?}", e),
            }
        }
        Action::Read(fp) => {
            let clearance = labeled::buckle::Buckle::parse("faasten,T").unwrap();
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());
            snapfaas::fs::utils::set_clearance(clearance);

            let path = snapfaas::fs::path::Path::parse(&fp.path).unwrap();
            match snapfaas::fs::utils::read(&fs, path) {
                Ok(mut data) => {
                    stdout().write(&mut data).unwrap();
                }
                Err(e) => log::warn!("Failed read. {:?}", e),
            }
        }
        Action::Delete(fp) => {
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

            let path = snapfaas::fs::path::Path::parse(&fp.path).unwrap();
            println!(
                "{}",
                snapfaas::fs::utils::delete(&fs, path.parent().unwrap(), path.file_name().unwrap())
                    .is_ok()
            );
        }
        Action::Mkdir(md) => {
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

            let dest = snapfaas::fs::path::Path::parse(&md.path).unwrap();
            let label = labeled::buckle::Buckle::parse(&md.label).unwrap();
            println!(
                "{}",
                snapfaas::fs::utils::create_directory(
                    &fs,
                    dest.parent().unwrap(),
                    dest.file_name().unwrap(),
                    label
                )
                .is_ok()
            );
        }
        Action::CreateBlob(cb) => {
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

            let mut file = std::fs::File::open(&cb.src)?;
            let dest = snapfaas::fs::path::Path::parse(&cb.dest).unwrap();
            let label = labeled::buckle::Buckle::parse(&cb.label).unwrap();
            let mut blobstore: blobstore::Blobstore<Sha256> =
                snapfaas::blobstore::Blobstore::default();
            let mut blob = blobstore.create().unwrap();
            let _ = std::io::copy(&mut file, &mut blob);
            let blob = blobstore.save(blob).unwrap();
            println!(
                "{}",
                snapfaas::fs::utils::create_blob(
                    &fs,
                    dest.parent().unwrap(),
                    dest.file_name().unwrap(),
                    label,
                    blob.name
                )
                .is_ok()
            );
        }
    }
    Ok(())
}
