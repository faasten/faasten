//! Faasten file system preparer
//!
//! The preparer installs supported kernels and runtime images in the directory ``home:^T,faasten''.
//! Kernels and runtime images are stored as blobs.

use clap::{Parser, Subcommand};
use jwt::{PKeyWithDigest, SignWithKey};
use labeled::buckle::{Buckle, Component};
use openssl::pkey::PKey;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use snapfaas::{
    blobstore, cli,
    fs::{BackingStore, FS},
};
use std::{
    io::{stdout, Write},
    time::SystemTime,
};

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

#[derive(Parser, Debug)]
struct Jwt {
    #[arg(value_name = "Component")]
    component: String,
    #[arg(short = 'k', long, value_name = "PATH")]
    secret_key: std::ffi::OsString,
}

#[derive(Parser, Debug)]
struct GenKeypair {
    /// Faasten path to store the private key
    #[arg(
        long,
        value_name = "FAASTEN_PATH",
        default_value = "home:<faasten,faasten>:private_key"
    )]
    private_key: String,
    /// Faasten path to store the private key
    #[arg(
        long,
        value_name = "FAASTEN_PATH",
        default_value = "home:<T,faasten>:public_key"
    )]
    public_key: String,
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
    /// Generate JWT
    Jwt(Jwt),
    /// Generate a key pair and store them in Faasten storage
    GenKeypair(GenKeypair),
}

pub fn main() -> std::io::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    let fs: FS<Box<dyn BackingStore>> = if let Some(tikv_pds) = cli.store.tikv {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let client = rt.block_on(async { tikv_client::RawClient::new(tikv_pds).await.unwrap() });
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
            snapfaas::fs::bootstrap::prepare_fs(&fs, &bs.yaml).expect("");
        }
        Action::UpdatePython(ui) => {
            snapfaas::fs::bootstrap::update_python(&fs, blobstore, &ui.path);
        }
        Action::UpdateFsutil(ui) => {
            snapfaas::fs::bootstrap::update_fsutil(&fs, blobstore, &ui.path);
        }
        Action::List(fp) => {
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

            let path = snapfaas::fs::path::Path::parse(&fp.path).unwrap();
            match fs.list_dir(path) {
                Ok(entries) => {
                    for (name, dent) in entries {
                        println!("{}\t{:?}", name, dent);
                    }
                }
                Err(e) => log::warn!("Failed list. {:?}", e),
            }
        }
        Action::FacetedList(fp) => {
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

            let path = snapfaas::fs::path::Path::parse(&fp.path).unwrap();
            match fs.list_faceted(path, &Buckle::top()) {
                Ok(entries) => {
                    for (label, _directory) in entries {
                        println!("{:?}", label);
                    }
                }
                Err(e) => log::warn!("Failed list. {:?}", e),
            }
        }
        Action::Read(fp) => {
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

            let path = snapfaas::fs::path::Path::parse(&fp.path).unwrap();
            match fs.read_file(path) {
                Ok(data) => {
                    stdout().write(&data).unwrap();
                }
                Err(e) => log::warn!("Failed read. {:?}", e),
            }
        }
        Action::Delete(fp) => {
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

            let path = snapfaas::fs::path::Path::parse(&fp.path).unwrap();
            println!(
                "{}",
                fs.rm(path.parent().unwrap(), &path.file_name().unwrap())
                    .is_ok()
            );
        }
        Action::Mkdir(md) => {
            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

            let dest = snapfaas::fs::path::Path::parse(&md.path).unwrap();
            let label = labeled::buckle::Buckle::parse(&md.label).unwrap();

            let new_dir = fs.create_directory(label);
            println!(
                "{}",
                fs.link(dest.parent().unwrap(), dest.file_name().unwrap(), new_dir)
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
                snapfaas::fs::utils::create_or_update_blob(
                    &fs,
                    dest.parent().unwrap(),
                    dest.file_name().unwrap(),
                    label,
                    blob.name
                )
                .is_ok()
            );
        }
        Action::GenKeypair(gkp) => {
            use openssl::ec::{EcGroup, EcKey};
            use openssl::error::ErrorStack;
            use openssl::nid::Nid;
            struct KeyPairPem {
                private_key_pem: Vec<u8>,
                public_key_pem: Vec<u8>,
            }
            fn generate_ec_keys() -> Result<KeyPairPem, ErrorStack> {
                // Create a new EC group object for the prime256v1 curve
                let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;

                // Generate the EC key pair
                let ec_key = EcKey::generate(&group)?;

                // Serialize the private key to PEM
                let private_key_pem = ec_key.private_key_to_pem()?;

                // We need to explicitly create a public key here
                let public_key = ec_key.public_key();
                let ec_key_with_pub = EcKey::from_public_key(&group, public_key)?;
                let public_key_pem = ec_key_with_pub.public_key_to_pem()?;
                Ok(KeyPairPem {
                    private_key_pem,
                    public_key_pem,
                })
            }
            let KeyPairPem {
                private_key_pem,
                public_key_pem,
            } = generate_ec_keys()?;

            snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());
            let private_dest = snapfaas::fs::path::Path::parse(&gkp.private_key).unwrap();
            let private_label = labeled::buckle::Buckle::parse("faasten,faasten").unwrap();
            println!(
                "{}",
                snapfaas::fs::utils::create_or_update_file(
                    &fs,
                    private_dest.parent().unwrap(),
                    private_dest.file_name().unwrap(),
                    private_label,
                    private_key_pem,
                )
                .is_ok()
            );
            let public_dest = snapfaas::fs::path::Path::parse(&gkp.public_key).unwrap();
            let public_label = labeled::buckle::Buckle::parse("T,faasten").unwrap();
            println!(
                "{}",
                snapfaas::fs::utils::create_or_update_file(
                    &fs,
                    public_dest.parent().unwrap(),
                    public_dest.file_name().unwrap(),
                    public_label,
                    public_key_pem,
                )
                .is_ok()
            );
        }
        Action::Jwt(jwt) => {
            let private_key_bytes = std::fs::read(jwt.secret_key)?;
            let pkey = PKey::private_key_from_pem(private_key_bytes.as_slice())?;

            let component = Buckle::parse(format!("{},T", jwt.component).as_str())
                .unwrap()
                .secrecy;
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            #[derive(Clone, Serialize, Deserialize, Debug)]
            struct Claims {
                pub alg: String,
                pub iat: u64,
                pub exp: u64,
                pub sub: Component,
            }

            let claims = Claims {
                alg: "ES256".to_string(),
                iat: now,
                exp: now + 10 * 60,
                sub: component,
            };
            let key = PKeyWithDigest {
                key: pkey,
                digest: openssl::hash::MessageDigest::sha256(),
            };
            let token = claims.sign_with_key(&key).unwrap();
            println!("{}", token);
        }
    }
    Ok(())
}
