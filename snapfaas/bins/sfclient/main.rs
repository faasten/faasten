#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, Arg, SubCommand};
use labeled::buckle::{self, Clause};
use snapfaas::request::LabeledInvoke;
use snapfaas::{fs, request, syscalls, vm};
use std::net::TcpStream;
use std::io::{Read, stdin};

fn main() {
    let cmd_arguments = App::new("SnapFaaS CLI Client")
        .version(crate_version!())
        .author(crate_authors!())
        .about("All subcommands act as the principal PRINCIPAL.")
        .arg(
            Arg::with_name("principal")
                .value_name("PRINCIPAL")
                .long("principal")
                .takes_value(true)
                .required(true)
                .help("Comma-separated principal string"),
        )
       .subcommand(
           SubCommand::with_name("invoke")
           .about("Act as the principal PRINCIPAL and invoke the gate at GATE")
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
               Arg::with_name("path")
                   .value_name("GATE")
                   .long("gate")
                   .takes_value(true)
                   .required(true)
                   .value_delimiter(":")
                   .help("Colon separated path of the gate to be invoked. Sfclient tries to parse each component first as a Buckle label. If failure, sfclient uses it as it is."),
           )
       )
       .subcommand(
           SubCommand::with_name("newgate")
           .about("Act as the principal PRINCIPAL and create a gate at GATE from the function name.")
            .arg(
                Arg::with_name("path")
                    .value_name("GATE")
                    .long("gate")
                    .takes_value(true)
                    .required(true)
                    .value_delimiter(":")
                    .help("Colon separated path of the gate to be created. Sfclient tries to parse each component first as a Buckle label. If failure, sfclient uses it as it is."),
            )
            .arg(
                Arg::with_name("policy")
                    .value_name("POLICY")
                    .long("policy")
                    .takes_value(true)
                    .required(true)
                    .help("A parsable Buckle string piggybacking the gate's policy. The secrecy should be the gate's privilege. The integrity should be the gate's integrity."),
            )
            .arg(
                Arg::with_name("function")
                    .value_name("FUNCTION NAME")
                    .long("function")
                    .takes_value(true)
                    .required(true)
                    .help("Function name string."),
            )
       )
       .get_matches();


    let principal: Vec<&str> = cmd_arguments.value_of("principal").unwrap().split(',').collect();
    let fs = snapfaas::fs::FS::new(&*snapfaas::labeled_fs::DBENV);
    fs::utils::clear_label();
    fs::utils::set_my_privilge([Clause::new_from_vec(vec![principal])].into());
    match cmd_arguments.subcommand() {
        ("invoke", Some(sub_m)) => {
            let addr = sub_m.value_of("server address").unwrap();
            let path: Vec<&str> = sub_m.values_of("path").unwrap().collect();
            let path = path.iter().map(|s| {
                // try parse it as a facet, if failure, as a regular name
                if let Ok(l) = buckle::Buckle::parse(s) {
                    let f = vm::buckle_to_pblabel(&l);
                    syscalls::PathComponent{ component: Some(syscalls::path_component::Component::Facet(f)) }
                } else {
                    syscalls::PathComponent{ component: Some(syscalls::path_component::Component::Dscrp(s.to_string())) }
                }
            }).collect();
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
        },
        ("newgate", Some(sub_m)) => {
            let function = sub_m.value_of("function").unwrap().to_string();
            let policy = buckle::Buckle::parse(sub_m.value_of("policy").unwrap());
            if policy.is_err() {
                eprintln!("Bad gate policy.");
                return;
            }
            let policy = policy.unwrap();
            let mut path = sub_m.values_of("path").unwrap().collect::<Vec<&str>>();
            if let Some(name) = path.pop() {
                let base_dir = path.iter().map(|s| {
                    // try parse it as a facet, if failure, as a regular name
                    if let Ok(l) = buckle::Buckle::parse(s) {
                        let f = vm::buckle_to_pblabel(&l);
                        syscalls::PathComponent{ component: Some(syscalls::path_component::Component::Facet(f)) }
                    } else {
                        syscalls::PathComponent{ component: Some(syscalls::path_component::Component::Dscrp(s.to_string())) }
                    }
                }).collect();

                // TODO: use global function name for now
                if let Err(e) = fs::utils::create_gate(&fs, &base_dir, name.to_string(), policy, function) {
                    eprintln!("Cannot create the gate at the path {:?}", e);
                }
            } else {
                eprintln!("Bad path");
            }
        },
        (&_, _) => {
            eprintln!("{}", cmd_arguments.usage());
        }
    }
}
