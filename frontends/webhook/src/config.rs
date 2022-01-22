use serde::Deserialize;
use serde_yaml;
use std::collections::BTreeMap;
use std::fs::File;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub secret: Option<String>,
    pub repos: BTreeMap<String, Repo>,
}

impl Config {
    pub fn new(path: &str) -> Self {
        serde_yaml::from_reader(File::open(path).expect("Failed to open app config"))
            .expect("Bad app config yaml file")
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Repo {
    pub organization: Option<String>,
    pub token: Option<String>,
    pub secret: Option<String>,
}
