pub mod request;
pub mod worker;
pub mod workerpool;
pub mod message;
pub mod gateway;
pub mod configs;
pub mod controller;

use std::string::String;

const LOCAL_FILE_URL_PREFIX: &str = "file://localhost";
/// check if a string is a url string
/// TODO: maybe a more comprehensive check is needed but low priority
pub fn check_url(path: &str) -> bool {
    return path.starts_with("file://")
         | path.starts_with("http://")
         | path.starts_with("https://")
         | path.starts_with("ftp://");
}


/// Check is a path is local filesystem path. If yes,
/// append file://localhost/ to local filesystem paths and expand ., .. and ~.
/// TODO: maybe a more comprehensive implementation is needed but low priority
pub fn convert_fs_path_to_url (path: &str) -> String {
    if check_url(path) {
        return path.to_string();
    }
    let mut url = String::from(LOCAL_FILE_URL_PREFIX);
    url.push_str(&shellexpand::tilde(path));

    return url;
}
