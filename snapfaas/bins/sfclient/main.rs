#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, Arg};
use snapfaas::request;
use std::net::TcpStream;
use std::io::{Read, stdin};
use std::collections::HashMap;
use labeled::dclabel::DCLabel;

fn main() -> std::io::Result<()> {
    let cmd_arguments = App::new("SnapFaaS CLI Client")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Make a request to SnapFaaS")
        .arg(
            Arg::with_name("server address")
                .value_name("[ADDR:]PORT")
                .long("server")
                .short("s")
                .takes_value(true)
                .required(true)
                .help("Address on which SnapFaaS is listening for connections"),
        )
        .arg(
            Arg::with_name("function")
                .value_name("FUNCTION")
                .long("function")
                .short("f")
                .takes_value(true)
                .required(true)
                .help("Function name"),
        )
        .get_matches();


    let addr = cmd_arguments.value_of("server address").unwrap();
    let function = cmd_arguments.value_of("function").unwrap().to_string();
    let mut input = Vec::new();
    stdin().read_to_end(&mut input)?;
    let payload = serde_json::from_slice(&input)?;
    let request = request::Request {
        function,
        payload,
        label: DCLabel::public(),
        data_handles: HashMap::new(),
    };

    let mut connection = TcpStream::connect(addr)?;
    request::write_u8(&request.to_vec(), &mut connection)?;
    input = request::read_u8(&mut connection)?;
    let response: request::Response = serde_json::from_slice(&input)?;
    println!("{:?}", response);
    Ok(())
}
