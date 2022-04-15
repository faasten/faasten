//! ResourceManager and function configuration
//! In-memory data structures that represent controller configuration and
//! function configurations
use serde::Deserialize;
use serde_yaml;
use url::Url;
use log::{info, debug};

use std::fs::File;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::convert_fs_path_to_url;

#[derive(Deserialize, Debug, Default)]
pub struct ResourceManagerConfig {
    pub allow_network: bool,
    pub firerunner_path: String,
    pub kernel_path: String,
    pub runtimefs_dir: String,
    #[serde(default)]
    pub appfs_dir: Option<String>,
    #[serde(default)]
    pub snapshot_dir: Option<String>,
    pub functions: BTreeMap<String, FunctionConfig>,
}

impl ResourceManagerConfig {
    /// Create in-memory ResourceManagerConfig struct from a YAML file
    pub fn new(path: &str) -> Self {
        // TODO: Currently only supports file://localhost urls
        let config_url = convert_fs_path_to_url(path).expect("Invalid configuration file path");
        info!("Using controller config: {}", config_url);

        ResourceManagerConfig::initialize(&config_url)
    }

    fn initialize(config_url: &str) -> Self {
        if let Ok(config_url) = Url::parse(config_url) {
            // populate a ResourceManagerConfig struct from the yaml file
            if let Ok(f) = File::open(config_url.path()) {
                let config: serde_yaml::Result<ResourceManagerConfig> = serde_yaml::from_reader(f);
                match config {
                    Ok(mut config) => {
                        ResourceManagerConfig::convert_to_url(&mut config);
                        ResourceManagerConfig::build_full_path_fs_images(&mut config);
                        debug!("ResourceManager config: {:?}", config);
                        config
                    },
                    Err(e) => panic!("Invalid YAML file {:?}", e)
                }
            } else {
                panic!("Invalid local path to config file");
            }

        } else {
            panic!("Invalid URL to config file")
        }
    }

    fn convert_to_url(config: &mut ResourceManagerConfig) {
        config.kernel_path = convert_fs_path_to_url(&config.kernel_path)
            .expect("Invalid kernel path");
        config.runtimefs_dir = convert_fs_path_to_url(&config.runtimefs_dir)
            .expect("Invalid runtimefs directory");
        config.appfs_dir = config.appfs_dir.as_ref().map(|dir| convert_fs_path_to_url(dir)
            .expect("Invalid appfs directory"));
        config.snapshot_dir = config.snapshot_dir.as_ref().map(|dir| convert_fs_path_to_url(dir)
            .expect("Invalid snapshot directory"));
    }

    fn build_full_path_fs_images(config: &mut ResourceManagerConfig) {
        let runtimefs_base = config.get_runtimefs_base();
        let maybe_appfs_base = config.get_appfs_base();
        let maybe_snapshot_base = config.get_snapshot_base();
        let kernel_path = &config.kernel_path;
        for app in config.functions.values_mut() {
            // build full path to the runtimefs
            app.runtimefs = [ &runtimefs_base, &app.runtimefs ]
                .iter().collect::<PathBuf>().to_str().unwrap().to_string();
            // build full path to the appfs
            app.appfs = app.appfs.as_ref()
                .map(|d| [ maybe_appfs_base.as_ref().expect("Appfs directory not specified"), d ]
                     .iter().collect::<PathBuf>().to_str().unwrap().to_string());
            app.load_dir = app.load_dir.as_ref().map(|s| s.split(',').collect::<Vec<&str>>().iter()
                    .map(|s| [ maybe_snapshot_base.as_ref().expect("Snapshot directory not specified").as_str(), s ]
                         .iter().collect::<PathBuf>().to_str().unwrap().to_string())
                    .collect::<Vec<String>>().join(","));
            // TODO: currently all apps use the same kernel
            app.kernel = Url::parse(kernel_path)
                .expect("Bad kernel path URL").path().to_string();
            // use `firerunner`'s default DEFAULT_KERNEL_CMDLINE
            // defined in firecracker/vmm/lib.rs
            //app.cmdline = None;
            // `snapctr` does not support generate snapshots
            app.dump_dir = None;
        }
    }

    pub fn get_runtimefs_base(&self) -> String {
        Url::parse(&self.runtimefs_dir).expect("invalid runtimefs dir from url").path().to_string()
    }

    pub fn get_appfs_base(&self) -> Option<String> {
        self.appfs_dir.as_ref().map(|d| Url::parse(&d).expect("invalid runtimefs dir from url").path().to_string())
    }

    pub fn get_snapshot_base(&self) -> Option<String> {
        self.snapshot_dir.as_ref().map(|d| Url::parse(&d).expect("invalid snapshot dir from url").path().to_string())
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct FunctionConfig {
    /// enable network
    #[serde(default)]
    pub network: bool,
    /// path to runtimefs
    pub runtimefs: String,
    /// path to appfs
    #[serde(default)]
    pub appfs: Option<String>,
    /// VM vcpu count
    pub vcpus: u64,
    /// VM memory size
    pub memory: usize,
    pub concurrency_limit: usize, // not in use
    /// base snapshot
    #[serde(default)]
    pub load_dir: Option<String>,
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
    /// directory to store the working set
    #[serde(default)]
    pub dump_ws: bool,
    /// load the working set
    #[serde(default)]
    pub load_ws: bool,
}

impl Default for FunctionConfig {
    fn default() -> Self {
        FunctionConfig {
            network: false,
            kernel: String::new(),
            runtimefs: String::new(),
            appfs: None,
            vcpus: 1,
            memory: 128,
            concurrency_limit: 1, // not in use
            load_dir: None,
            //diff_dirs: None,
            copy_base: false,
            copy_diff: true,
            cmdline: None,
            dump_dir: None,
            dump_ws: false,
            load_ws: false,
        }
    }
}
