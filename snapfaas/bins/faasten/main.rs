#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, SubCommand, Arg};

const HTTP_PREFIX:  &str = "http://"; // TODO make https work
const DEFAULT_IP:   &str = "127.0.0.1";
const COLON:        &str = ":";
const DEFAULT_PORT: &str = "8080";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let cmd_arguments = App::new("faasten")
      .version(crate_version!())
      .author(crate_authors!())
      .subcommand(
        SubCommand::with_name("server_ip")
          .about("Specify server IP address")
          .arg(Arg::with_name("SERVER_IP").required(true))
      )
      .subcommand(
        SubCommand::with_name("server_port")
          .about("Specify server port")
          .arg(Arg::with_name("SERVER_PORT").required(true))
      )
      .subcommand(
        SubCommand::with_name("get")
          .about("Retrives contents from <PATH> subject to client privileges: if <PATH> is a file, file contents are returned; if <PATH> is a directory or faceted store, the entries contained in it are returned. Note that <PATH> must be an absolute path starting with /.")
          .arg(Arg::with_name("PATH").index(1).required(true))
      )
      .subcommand(
        SubCommand::with_name("delete")
          .about("Deletes contents at <PATH>, where <PATH> must be an absolute path to a file, directory, or faceted store.")
          .arg(Arg::with_name("PATH").index(1).required(true))
      )
      .subcommand(
        SubCommand::with_name("put")
          .about("Creates an entry of type <TYPE> at <PATH>, where <TYPE> can be a file, directory, or faceted store.")
          .arg(Arg::with_name("PATH").index(1).required(true))
          .arg(Arg::with_name("TYPE").required(true)) // TODO encoding?
      )
      .subcommand(
        SubCommand::with_name("post")
          .about("Writes <CONTENTS> to the entry at <PATH>, (TODO behavior?) if <PATH> does not already exist.")
          .arg(Arg::with_name("PATH").index(1).required(true))
          .arg(Arg::with_name("CONTENTS").required(true))
      )
      .get_matches();

  let mut server_ip = DEFAULT_IP;
  let mut server_port = DEFAULT_PORT;
  // FIXME do this without separate match statements
  match cmd_arguments.subcommand() {
    ("server_ip", Some(sub_m)) => {
      server_ip = sub_m.value_of("SERVER_IP").unwrap()
    },
    ("server_port", Some(sub_m)) => {
      server_port = sub_m.value_of("SERVER_PORT").unwrap()
    },
    (&_, _) => {}
  };

  let base_addr: &str = &vec![HTTP_PREFIX, server_ip, COLON, server_port].concat();

  let client = reqwest::Client::new();
  match cmd_arguments.subcommand() {
    ("get", Some(sub_m)) => {
      let path = sub_m.value_of("PATH").unwrap();
      let full_addr: String = vec![base_addr, path].concat();
      let response = client.get(full_addr)
        .send()
        .await?;
      println!("Status: \n{:?}", response.status());
      let body = response.text().await?;
      println!("Body: \n{}", body);
    },
    ("delete", Some(sub_m)) => {
      let path = sub_m.value_of("PATH").unwrap();
      let full_addr: String = vec![base_addr, path].concat();
      let response = client.delete(full_addr)
        .send()
        .await?;
      println!("Status: \n{:?}", response.status());
    },
    ("put", Some(sub_m)) => {
      let path = sub_m.value_of("PATH").unwrap();
      let full_addr: String = vec![base_addr, path].concat();
      let entry_type: String = sub_m.value_of("TYPE").unwrap().to_string();
      let response = client.put(full_addr)
        .body(entry_type)
        .send()
        .await?;
      println!("Status: \n{:?}", response.status());
    },
    ("post", Some(sub_m)) => {
      let path = sub_m.value_of("PATH").unwrap();
      let full_addr: String = vec![base_addr, path].concat();
      let contents: String = sub_m.value_of("CONTENTS").unwrap().to_string();
      let response = client.post(full_addr)
        .body(contents)
        .send()
        .await?;
      println!("Status: \n{:?}", response.status());
    },
    (&_, _) => {
      eprintln!("{}", cmd_arguments.usage());
    }
  }
  Ok(())
}
