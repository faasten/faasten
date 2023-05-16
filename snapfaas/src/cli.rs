//! Definitions of common CLI arguments

use clap::Args;

#[derive(Args, Debug)]
#[group(multiple = true, required = true)]
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
    /// MAC address of the microVM's network device
    #[arg(long, value_name = "MAC", group = "network", requires = "tap")]
    pub mac: Option<String>,
    /// Name of the tap device that backs the microVM's network device
    #[arg(long, value_name = "NAME", group = "network")]
    pub tap: Option<String>,
    /// Directory to load the snapshot from
    #[arg(long, value_name = "PATH", group = "load_snapshot")]
    pub load_dir: Option<String>,
    /// If present, load the working set
    #[arg(long, group = "load_snapshot")]
    pub load_ws: bool,
    /// If present, restore the base memory snapshot by copying
    #[arg(long, group = "load_snapshot")]
    pub copy_base_memory: bool,
    /// If present, restore the diff memory snapshot by copying
    #[arg(long, group = "load_snapshot")]
    pub copy_diff_memory: bool,
    /// Directory to create the snapshot in
    #[arg(long, value_name = "PATH", group = "dump_snapshot")]
    pub dump_dir: Option<String>,
    /// If present, dump the working set
    #[arg(long, group = "dump_snapshot")]
    pub dump_ws: bool,
    /// If present, open base memory snapshot with O_DIRECT
    #[arg(long, group = "odirect", requires = "load_snapshot")]
    pub odirect_base: bool,
    /// If present, don't open diff memory snapshot with O_DIRECT
    #[arg(long, group = "odirect", requires = "load_snapshot")]
    pub no_odirect_diff: bool,
    /// If present, don't open rootfs with O_DIRECT (required when using tmpfs)
    #[arg(long, group = "odirect")]
    pub no_odirect_root: bool,
    /// If present, don't open appfs with O_DIRECT (required when using tmpfs)
    #[arg(long, group = "odirect")]
    pub no_odirect_app: bool,
}

#[derive(Args, Debug)]
#[group(required = true)]
pub struct Store {
    /// Space delimited addresses of TiKV PDs
    #[arg(long, value_name = "ADDR:PORT")]
    pub tikv: Option<Vec<String>>,
    /// Path of the LMDB directory
    #[arg(long, value_name = "PATH")]
    pub lmdb: Option<String>,
}
