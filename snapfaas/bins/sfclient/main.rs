#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, Arg};
use labeled::buckle::{self, Clause};
use snapfaas::request::LabeledInvoke;
use snapfaas::{fs, request, syscalls, vm};
use std::net::TcpStream;
use std::io::{Read, stdin};

fn main() {
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
        .arg(
            Arg::with_name("principal")
                .value_name("PRINCIPAL")
                .long("principal")
                .takes_value(true)
                .required(true)
                .help("Comma-separated principal string"),
        )
        .get_matches();


    let addr = cmd_arguments.value_of("server address").unwrap();
    let principal: Vec<&str> = cmd_arguments.value_of("principal").unwrap().split(',').collect();
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
    let fs = snapfaas::fs::FS::new(&*snapfaas::labeled_fs::DBENV);
    fs::utils::clear_label();
    fs::utils::set_my_privilge([Clause::new_from_vec(vec![principal])].into());
    let gate = match fs::utils::read_path(&fs, &path) {
        Ok(fs::DirEntry::Gate(g)) => fs.invoke_gate(&g).ok(),
        _ => None,
    };
    if gate.is_none() {
        eprintln!("Cannot invoke the gate.");
        return;
    }
    let mut input = Vec::new();
    stdin().read_to_end(&mut input).unwrap();
    let payload = serde_json::from_slice(&input).unwrap();
    let request = LabeledInvoke {
        gate: gate.unwrap(),
        label: fs::utils::get_current_label(),
        payload,
    };

    let mut connection = TcpStream::connect(addr).unwrap();
    request::write_u8(serde_json::to_vec(&request).unwrap().as_ref(), &mut connection).unwrap();
    input = request::read_u8(&mut connection).unwrap();
    let response: request::Response = serde_json::from_slice(&input).unwrap();
    println!("{:?}", response);
}
