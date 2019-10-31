use serde::Deserialize;
use serde_yaml;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::fs::File;
use url::{Url, ParseError};
use log::{error, warn, info};

#[derive(Deserialize, Debug)]
pub struct ControllerConfig {
    pub kernel_path: String,
    pub kernel_boot_args: String,
}

impl ControllerConfig {

    pub fn new(config_file: &str) -> ControllerConfig {

        if let Ok(config_url) = Url::parse(config_file) {
            let config_path = Path::new(config_url.path());
            // populate a ControllerConfig struct from the yaml file
            // TODO: currently only local file is supported
            if let Ok(config) = File::open(config_path) {
                let config: serde_yaml::Result<ControllerConfig> = serde_yaml::from_reader(config);
                if let Ok(config) = config {
                    return config;
                } else {
                    warn!("Invalid YAML file");
                }
            } else {
                warn!("Invalid local path to config file");
            }

        } else {
            warn!("Invalid URL to config file")
        }

        return ControllerConfig {
            kernel_path: "".to_string(),
            kernel_boot_args: "".to_string(),
        };
    }

}


// represents an in-memory function config store
#[derive(Clone)]
pub struct Configuration {
    pub configs: BTreeMap<String, FunctionConfig>,
    runtimefs_dir: PathBuf,
    appfs_dir: PathBuf,
}

impl Configuration {
    pub fn new<R: AsRef<Path>, A: AsRef<Path>>(runtimefs_dir: R, appfs_dir: A, config_file: File) -> Configuration {
        let mut config = Configuration {
            configs: BTreeMap::new(),
            runtimefs_dir: [runtimefs_dir].iter().collect(),
            appfs_dir: [appfs_dir].iter().collect(),
        };

        let apps: serde_yaml::Result<Vec<FunctionConfig>> = serde_yaml::from_reader(config_file);
        for app in apps.unwrap() {
            config.insert(app);
        }

        return config;
    }

    pub fn insert(&mut self, config: FunctionConfig) {
        self.configs.insert(config.name.clone(), config);
    }

    pub fn get(&self, name: &String) -> Option<FunctionConfig> {
        self.configs.get(name).map(|c| {
            FunctionConfig {
                name: c.name.clone(),
                runtimefs: [self.runtimefs_dir.clone(), c.runtimefs.clone()].iter().collect(),
                appfs: [self.appfs_dir.clone(), c.appfs.clone()].iter().collect(), 
                vcpus: c.vcpus,
                memory: c.memory,
                concurrency_limit: c.concurrency_limit,
                load_dir: c.load_dir.clone(), 
            }
        })
    }

    pub fn num_func(&self) -> usize {
        self.configs.len()
    }

    pub fn exist(&self, name: &String) -> bool {
        self.configs.contains_key(name)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct FunctionConfig {
    pub name: String,
    pub runtimefs: PathBuf,
    pub appfs: PathBuf,
    pub vcpus: u64,
    pub memory: usize,
    pub concurrency_limit: usize,
    pub load_dir: PathBuf,
}

