use serde::Deserialize;
use serde_yaml;
use std::collections::BTreeMap;
use std::fs::File;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub repos: BTreeMap<String, Repo>,
}

impl Config {
    pub fn new(path: &str) -> Self {
        let repos_vec : Vec<Repo> =
            serde_yaml::from_reader(File::open(path).expect("Failed to open app config"))
            .expect("Bad app config yaml file");
        let mut repos = BTreeMap::new();
        for repo in repos_vec {
            repos.insert(repo.full_name.clone(), repo);
        }
        Config { repos }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Repo {
    pub organization: Option<String>,
    pub full_name: String,
    pub token: Option<String>,
    pub secret: Option<String>,
}
