#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, Arg, SubCommand};
use labeled::Label;
use labeled::buckle::{self, Clause, Buckle};
use serde_json::json;
use snapfaas::{fs, syscalls, vm, sched};
use std::collections::HashMap;
use std::net::TcpStream;
use std::io::{stdin, self, Write, Read};
use std::time::{self, Duration};

fn parse_path_vec(mut path: Vec<&str>) -> Vec<syscalls::PathComponent> {
    if path[0] == "" {
        path.remove(0);
    }
    path.iter().map(|s| {
        // try parse it as a facet, if failure, as a regular name
        if let Ok(l) = buckle::Buckle::parse(s) {
            let f = vm::buckle_to_pblabel(&l);
            syscalls::PathComponent{ component: Some(syscalls::path_component::Component::Facet(f)) }
        } else {
            syscalls::PathComponent{ component: Some(syscalls::path_component::Component::Dscrp(s.to_string())) }
        }
    }).collect()
}

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
        .arg(
            Arg::with_name("stat")
                .value_name("STAT LOG")
                .long("stat")
                .takes_value(true)
                .required(false)
                .help("file path to write stat"),
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
                   .value_name("GATE PATH")
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
               Arg::with_name("base-dir")
                   .value_name("BASE DIR")
                   .long("base-dir")
                   .takes_value(true)
                   .required(true)
                   .value_delimiter(":")
                   .help("Colon separated path of the directory to create the gate in. Sfclient tries to parse each component first as a Buckle label. If failure, sfclient uses it as it is."),
           )
           .arg(
               Arg::with_name("gate-name")
                   .value_name("GATE NAME")
                   .long("gate-name")
                   .takes_value(true)
                   .required(true)
                   .help("Name of the gate to be created"),
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
       .subcommand(
           SubCommand::with_name("ls")
           .about("list a directory")
           .arg(
               Arg::with_name("path")
               .index(1)
               .value_delimiter(":")
               .required(true)
               .help("A directory path."),
            )
       )
       .subcommand(
           SubCommand::with_name("facetedls")
           .about("list a faceted directory")
           .arg(
               Arg::with_name("path")
               .index(1)
               .value_delimiter(":")
               .required(true)
               .help("A faceted directory path."),
            )
       )
       .subcommand(
           SubCommand::with_name("write")
           .about("write a file")
           .arg(
               Arg::with_name("path")
               .index(1)
               .value_delimiter(":")
               .required(true)
               .help("A file path."),
            )
       )
       .subcommand(
           SubCommand::with_name("read")
           .about("read a file")
           .arg(
               Arg::with_name("path")
               .index(1)
               .value_delimiter(":")
               .required(true)
               .help("A file path."),
            )
       )
       .subcommand(
           SubCommand::with_name("del")
           .about("delete a path. act as unlink.")
           .arg(
               Arg::with_name("base-dir")
               .value_name("BASE DIR")
               .long("base-dir")
               .value_delimiter(":")
               .takes_value(true)
               .required(true)
               .help("Path of the base directory"),
            )
           .arg(
               Arg::with_name("name")
               .value_name("NAME")
               .long("name")
               .takes_value(true)
               .required(true)
               .help("Path of the base directory"),
            )
       )
       .subcommand(
           SubCommand::with_name("create")
           .about("create a directory")
           .arg(
               Arg::with_name("type")
               .index(1)
               .possible_values(&["dir", "file", "faceted"])
               .required(true)
               .help("Type of the object"),
            )
           .arg(
               Arg::with_name("base-dir")
               .value_name("BASE DIR")
               .long("base-dir")
               .value_delimiter(":")
               .takes_value(true)
               .required(true)
               .help("Path of the base directory"),
            )
           .arg(
               Arg::with_name("name")
               .value_name("NAME")
               .long("name")
               .takes_value(true)
               .required(true)
               .help("Path of the base directory"),
            )
           .arg(
               Arg::with_name("label")
               .value_name("LABEL")
               .long("label")
               .takes_value(true)
               .required_ifs(&[("type", "dir"), ("type", "file")])
               .help("Path of the base directory"),
            )
       )
       .get_matches();


    let principal: Vec<&str> = cmd_arguments.value_of("principal").unwrap().split(',').collect();
    let clearance = Buckle::new([Clause::new_from_vec(vec![principal.clone()])], true);
    let mut fs = snapfaas::fs::FS::new(&*snapfaas::labeled_fs::DBENV);
    fs::utils::clear_label();
    fs::utils::set_my_privilge([Clause::new_from_vec(vec![principal.clone()])].into());
    let mut elapsed = Duration::new(0, 0);
    let mut stat = fs::Metrics::default();
    match cmd_arguments.subcommand() {
        ("invoke", Some(sub_m)) => {
            let addr = sub_m.value_of("server address").unwrap();
            let path: Vec<&str> = sub_m.values_of("path").unwrap().collect();
            let path = parse_path_vec(path);
            let gate = match fs::utils::read_path(&fs, &path) {
                Ok(fs::DirEntry::Gate(g)) => fs.invoke_gate(&g).ok(),
                _ => None,
            };
            if gate.is_none() {
                eprintln!("Gate does not exist.");
                return;
            }
            for line in stdin().lines().map(|l| l.unwrap()) {
                let label = fs::utils::get_current_label();
                use prost::Message;
                let request = sched::message::LabeledInvoke {
                    invoke: Some(syscalls::Invoke { gate: path.clone(), payload: line }),
                    label: Some(vm::buckle_to_pblabel(&label)),
                    privilege: vm::component_to_pbcomponent(&[Clause::new_from_vec(vec![principal.clone()])].into()),
                };
                let mut connection = TcpStream::connect(addr).unwrap();
                sched::message::write_u8(&mut connection, &request.encode_to_vec()).unwrap();
                let buf = sched::message::read_u8(&mut connection).unwrap();
                let response = String::from_utf8(buf).unwrap();
                println!("{:?}", response);
            }
        },
        ("newgate", Some(sub_m)) => {
            let function = sub_m.value_of("function").unwrap().to_string();
            let name = sub_m.value_of("gate-name").unwrap().to_string();
            let policy = buckle::Buckle::parse(sub_m.value_of("policy").unwrap());
            if policy.is_err() {
                eprintln!("Bad gate policy.");
                return;
            }
            let policy = policy.unwrap();
            let base_dir = sub_m.values_of("base-dir").unwrap().collect::<Vec<&str>>();
            let base_dir = parse_path_vec(base_dir);

            // TODO: use global function name for now
            if let Err(e) = fs::utils::create_gate(&fs, &base_dir, name.to_string(), policy, function) {
                eprintln!("Cannot create the gate: {:?}", e);
            }
        },
        ("read", Some(sub_m)) => {
            fs::utils::taint_with_label(Buckle::new(fs::utils::my_privilege(), true));
            let path: Vec<&str> = sub_m.values_of("path").unwrap().collect();
            let path = parse_path_vec(path);
            let now = time::Instant::now();
            match fs::utils::read(&fs, &path) {
                Ok(data) => {
                    elapsed = now.elapsed();
                    stat = fs::metrics::get_stat();
                    if fs::utils::get_current_label().can_flow_to(&clearance) {
                        let _ = io::stdout().lock().write_all(&data).unwrap();
                        let _ = io::stdout().lock().flush();
                    } else {
                        eprintln!("Failed to read. Too tainted. {:?}", fs::utils::get_current_label());
                    }
                }
                Err(e) => { eprintln!("Failed to read. {:?}", e); },
            };
        }
        ("write", Some(sub_m)) => {
            let path: Vec<&str> = sub_m.values_of("path").unwrap().collect();
            let path = parse_path_vec(path);
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf).unwrap();
            let now = time::Instant::now();
            if let Err(e) = fs::utils::write(&mut fs, &path, buf) {
                eprintln!("Failed to write. {:?}", e);
            };
            elapsed = now.elapsed();
            stat = fs::metrics::get_stat();
        }
        ("ls", Some(sub_m)) => {
            let path: Vec<&str> = sub_m.values_of("path").unwrap().collect();
            let path = parse_path_vec(path);
            let now = time::Instant::now();
            match fs::utils::list(&fs, &path) {
                Ok(m) => {
                    let entries = m.keys().cloned().collect::<Vec<String>>();
                    if fs::utils::get_current_label().can_flow_to(&clearance) {
                        for entry in entries {
                            println!("{}", entry);
                        }
                    } else {
                        eprintln!("Failed to list. Too tainted. {:?}", fs::utils::get_current_label());
                    }
                }
                Err(e) => { eprintln!("Failed to list. {:?}", e); },
            };
            elapsed = now.elapsed();
            stat = fs::metrics::get_stat();
        },
        ("facetedls", Some(sub_m)) => {
            fs::utils::taint_with_label(Buckle::new(fs::utils::my_privilege(), true));
            let path: Vec<&str> = sub_m.values_of("path").unwrap().collect();
            let path = parse_path_vec(path);
            let now = time::Instant::now();
            match fs::utils::faceted_list(&fs, &path) {
                Ok(facets) => {
                    let entries = facets.iter().map(|(k, m)|
                        (k.clone(), m.keys().cloned().collect::<Vec<String>>())).collect::<HashMap<String, Vec<String>>>();
                    if fs::utils::get_current_label().can_flow_to(&clearance) {
                        for entry in entries {
                            println!("{:?}", entry);
                        }
                    } else {
                        eprintln!("Failed to list. Too tainted. {:?}", fs::utils::get_current_label());
                    }
                }
                Err(e) => { eprintln!("Failed to list. {:?}", e); },
            };
            elapsed = now.elapsed();
            stat = fs::metrics::get_stat();
        },
        ("del", Some(sub_m)) => {
            let base_dir = sub_m.values_of("base-dir").unwrap().collect();
            let name = sub_m.value_of("name").unwrap().to_string();
            let base_dir = parse_path_vec(base_dir);
            let now = time::Instant::now();
            if let Err(e) = fs::utils::delete(&fs, &base_dir, name) {
                eprintln!("Failed to delete. {:?}", e);
            }
            elapsed = now.elapsed();
            stat = fs::metrics::get_stat();
        },
        ("create", Some(sub_m)) => {
            let objtype = sub_m.value_of("type").unwrap();
            let base_dir = sub_m.values_of("base-dir").unwrap().collect();
            let name = sub_m.value_of("name").unwrap().to_string();
            let base_dir = parse_path_vec(base_dir);
            let label = sub_m.value_of("label").and_then(|s|
                buckle::Buckle::parse(s).ok()
            );

            let now = time::Instant::now();
            if objtype == "dir" {
                if label.is_none() {
                    eprintln!("Bad label");
                    return;
                }
                let label = label.unwrap();
                if let Err(e) = fs::utils::create_directory(&fs, &base_dir, name, label) {
                    eprintln!("Cannot create the directory. {:?}", e);
                    return;
                }
                elapsed = now.elapsed();
                stat = fs::metrics::get_stat();
            } else if objtype == "faceted" {
                if let Err(e) = fs::utils::create_faceted(&fs, &base_dir, name) {
                    eprintln!("Cannot create the faceted. {:?}", e);
                }
                elapsed = now.elapsed();
                stat = fs::metrics::get_stat();
            } else if objtype == "file" {
                if label.is_none() {
                    eprintln!("Bad label");
                    return;
                }
                let label = label.unwrap();
                if let Err(e) = fs::utils::create_file(&fs, &base_dir, name, label) {
                    eprintln!("Cannot create the file. {:?}", e);
                }
                elapsed = now.elapsed();
                stat = fs::metrics::get_stat();
            } else {
                panic!("{} is not a valid type.", objtype);
            }
        },
        (&_, _) => {
            eprintln!("{}", cmd_arguments.usage());
        }
    }
    let val = json!({
        "elapsed": elapsed,
        "stat": stat,
    });
    if let Some(fname) = cmd_arguments.value_of("stat") {
        let file = std::fs::File::create(fname).unwrap();
        serde_json::to_writer(file, &val).unwrap();
    } else {
        serde_json::to_writer_pretty(io::stdout(), &val).unwrap();
    }
}
