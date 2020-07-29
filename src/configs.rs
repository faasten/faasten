//! Controller and function configuration
//! In-memory data structures that represent controller configuration and
//! function configurations
use serde::Deserialize;
use serde_yaml;
use std::path::Path;
use std::fs::File;
use url::Url;
use log::info;
use crate::*;

pub const KERNEL_PATH: &str = "/etc/kernel/vmlinux-4.20.0";

#[derive(Deserialize, Debug)]
pub struct ControllerConfig {
    pub kernel_path: String,
    pub runtimefs_dir: String,
    pub appfs_dir: String,
    pub snapshot_dir: String,
    pub function_config: String,
}

impl ControllerConfig {

    /// Create in-memory ControllerConfig struct from a YAML file
    /// TODO: Currently only supports file://localhost urls
    pub fn new(path: &str) -> ControllerConfig {
        let config_url = convert_fs_path_to_url(path);
        info!("Using controller config: {}", config_url);

        return ControllerConfig::initialize(&config_url);
    }

    fn initialize(config_url: &str) -> ControllerConfig {
        if let Ok(config_url) = Url::parse(config_url) {
            let config_path = Path::new(config_url.path());
            // populate a ControllerConfig struct from the yaml file
            if let Ok(config) = File::open(config_path) {
                let config: serde_yaml::Result<ControllerConfig> = serde_yaml::from_reader(config);
                if let Ok(config) = config {
                    return config;
                } else {
                    panic!("Invalid YAML file");
                }
            } else {
                panic!("Invalid local path to config file");
            }

        } else {
            panic!("Invalid URL to config file")
        }
    }

    //pub fn set_kernel_path(&mut self, path: &str) {
    //    self.kernel_path = convert_fs_path_to_url(path);
    //}

    //pub fn set_kernel_boot_args(&mut self, args: &str) {
    //    self.kernel_boot_args= args.to_string();
    //}

    pub fn get_runtimefs_base(&self) -> String {
        Url::parse(&self.runtimefs_dir).expect("invalid runtimefs dir from url").path().to_string()
    }

    pub fn get_appfs_base(&self) -> String {
        Url::parse(&self.appfs_dir).expect("invalid runtimefs dir from url").path().to_string()
    }

    pub fn get_snapshot_base(&self) -> String {
        Url::parse(&self.snapshot_dir).expect("invalid snapshot dir from url").path().to_string()
    }

}

#[derive(Debug, Deserialize, Clone)]
pub struct FunctionConfig {
    /// function name used to distinguish functions
    pub name: String,
    /// path to runtimefs
    pub runtimefs: String,
    /// path to appfs
    pub appfs: String,
    /// VM vcpu count
    pub vcpus: u64,
    /// VM memory size
    pub memory: usize,
    pub concurrency_limit: usize, // not in use
    /// base snapshot
    #[serde(default)]
    pub load_dir: Option<String>,
    /// comma-separated list of diff snapshot directories
    #[serde(default)]
    pub diff_dirs: Option<String>,
    /// copy base snapshot memory dump
    #[serde(default)]
    pub copy_base: bool,
    /// copy diff snapshot memory dump
    #[serde(default)]
    pub copy_diff: bool,
    /// path to uncompressed kernel, only used by `fc_wrapper` not by `snapctr`
    /// `snapctr` set this field to the path specified in the configuration file
    #[serde(default)]
    pub kernel: String,
    /// boot command line arguments, only used by `fc_wrapper` not by `snapctr`
    /// `snapctr` set this field to None
    #[serde(default)]
    pub cmdline: Option<String>,
    /// directory to store snapshot, only used by `fc_wrapper` not by `snapctr`
    /// `snapctr` set this field to None
    #[serde(default)]
    pub dump_dir: Option<String>,
}

impl Default for FunctionConfig {
    fn default() -> Self {
        FunctionConfig {
            name: String::new(),
            kernel: KERNEL_PATH.to_string(),
            runtimefs: String::new(),
            appfs: String::new(),
            vcpus: 1,
            memory: 128,
            concurrency_limit: 1, // not in use
            load_dir: None,
            diff_dirs: None,
            copy_base: false,
            copy_diff: true,
            cmdline: None,
            dump_dir: None,
        }
    }
}
