#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, Arg};
use labeled::buckle;
use prost::Message;
use snapfaas::{request, syscalls, vm};
use std::net::TcpStream;
use std::io::{Read, stdin};

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
            Arg::with_name("gate")
                .value_name("GATE")
                .long("gate")
                .takes_value(true)
                .required(true)
                .help("Slash separated path of the gate to be invoked. Sfclient tries to parse each component first as a Buckle label. If failure, sfclient uses it as it is."),
        )
        .get_matches();


    let addr = cmd_arguments.value_of("server address").unwrap();
    let mut gate = cmd_arguments.value_of("function").unwrap();
    if let Some(p) = gate.strip_prefix('/') {
        gate = p;
    }
    let path = gate.split('/').map(|s| {
        // try parse it as a facet, if failure, as a regular name
        if let Ok(l) = buckle::Buckle::parse(s) {
            let f = vm::buckle_to_pblabel(&l);
            syscalls::PathComponent{ component: Some(syscalls::path_component::Component::Facet(f)) }
        } else {
            syscalls::PathComponent{ component: Some(syscalls::path_component::Component::Dscrp(s.to_string())) }
        }
    }).collect();
    let mut input = Vec::new();
    stdin().read_to_end(&mut input)?;
    let payload = serde_json::from_slice(&input)?;
    let request = syscalls::Invoke {
        gate: path,
        payload,
    };

    let mut connection = TcpStream::connect(addr)?;
    request::write_u8(&request.encode_to_vec(), &mut connection)?;
    input = request::read_u8(&mut connection)?;
    let response: request::Response = serde_json::from_slice(&input)?;
    println!("{:?}", response);
    Ok(())
}
