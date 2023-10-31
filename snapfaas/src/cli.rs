//! Definitions of common CLI arguments

use clap::{Args, Parser};

#[derive(Parser, Debug)]
pub struct VmConfig {
    /// MicroVM ID
    #[arg(long, default_value_t = 0)]
    pub id: u64,
    /// Path of the kernel
    #[arg(short, long, value_name = "PATH")]
    pub kernel: String,
    /// Kernel boot args (e.g. quiet,console=ttyS0)
    #[arg(long, value_name = "FLAG|KEY=VALUE,...")]
    pub kernel_args: Option<String>,
    /// Path of the root file-system
    #[arg(short, long, value_name = "PATH")]
    pub rootfs: String,
    /// Path of the app file-system, mounted at "/srv" in the microVM
    #[arg(long, value_name = "PATH")]
    pub appfs: Option<String>,
    /// Memory size in MB of the microVM
    #[arg(long, value_name = "MB", default_value_t = 128)]
    pub memory: u32,
    /// VCPU count of the microVM
    #[arg(long, value_name = "COUNT", default_value_t = 1)]
    pub vcpu: u32,
    /// CID of the microVM's vsock
    #[arg(long, value_name = "CID", default_value_t = 100)]
    pub vsock_cid: u32,
    #[command(flatten)]
    pub network: Network,
    #[command(flatten)]
    pub load: Load,
    #[command(flatten)]
    pub dump: Dump,
    /// If present, don't open rootfs with O_DIRECT (required when using tmpfs)
    #[arg(long)]
    pub no_odirect_root: bool,
    /// If present, don't open appfs with O_DIRECT (required when using tmpfs)
    #[arg(long)]
    pub no_odirect_app: bool,
}

#[derive(Args, Debug)]
#[group(required = false, multiple = true)]
pub struct Network {
    /// MAC address of the microVM's network device
    #[arg(long, value_name = "MAC", requires = "tap")]
    pub mac: Option<String>,
    /// Name of the tap device that backs the microVM's network device
    #[arg(long, value_name = "NAME")]
    pub tap: Option<String>,
}

#[derive(Args, Debug)]
#[group(required = false, multiple = true)]
pub struct Load {
    /// Directory to load the snapshot from
    #[arg(long, value_name = "PATH")]
    pub load_dir: Option<String>,
    /// If present, load the working set
    #[arg(long, requires = "load_dir")]
    pub load_ws: bool,
    /// If present, restore the base memory snapshot by copying
    #[arg(long, requires = "load_dir")]
    pub copy_base_memory: bool,
    /// If present, restore the diff memory snapshot by copying
    #[arg(long, requires = "load_dir")]
    pub copy_diff_memory: bool,
    /// If present, open base memory snapshot with O_DIRECT
    #[arg(long, requires = "load_dir")]
    pub odirect_base: bool,
    /// If present, don't open diff memory snapshot with O_DIRECT
    #[arg(long, requires = "load_dir")]
    pub no_odirect_diff: bool,
}

#[derive(Args, Debug)]
#[group(required = false, multiple = true, conflicts_with = "Load")]
pub struct Dump {
    /// Directory to create the snapshot in
    #[arg(long, value_name = "PATH")]
    pub dump_dir: Option<String>,
    /// If present, dump the working set
    #[arg(long, requires = "dump_dir")]
    pub dump_ws: bool,
}

#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
pub struct Store {
    /// Space delimited addresses of TiKV PDs
    #[arg(long, value_name = "ADDR:PORT")]
    pub tikv: Option<Vec<String>>,
    /// Path of the LMDB directory
    #[arg(long, value_name = "PATH")]
    pub lmdb: Option<String>,
}
