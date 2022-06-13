#[macro_use(crate_version, crate_authors)]
extern crate clap;
use clap::{App, Arg, SubCommand};
use labeled::dclabel::{self, DCLabel};
use std::io::{Read, Write};

use snapfaas::labeled_fs;

fn input_to_dclabel(si_clauses: [Vec<&str>; 2]) -> DCLabel {
    let mut components = Vec::new();
    for clauses in si_clauses {
        let component: dclabel::Component = if clauses[0].to_lowercase() == "true" {
            true.into()
        } else if clauses[0].to_lowercase() == "false" {
            false.into()
        } else {
            let mut s_vec = Vec::new();
            for clause in clauses {
                let c: Vec<String> = clause.split(",").map(|s| s.to_lowercase()).collect();
                s_vec.push(c);
            }
            s_vec.into()
        };
        components.push(component);
    }
    let secrecy = components.remove(0);
    let integrity = components.remove(0);
    DCLabel::new(secrecy, integrity)
}

fn input_to_endorsement(endorse: &str) -> DCLabel {
    if endorse.to_lowercase() == "false" {
        DCLabel::new(true, false)
    } else if endorse.to_lowercase() == "true" {
        DCLabel::new(true, true)
    } else {
        DCLabel::new(true, [[endorse.to_lowercase()]])
    }
}

fn main() {
    let cmd_arguments = App::new("sffs")
        .version(crate_version!())
        .author(crate_authors!())
        .about("This program is a wrapper over the labeled_fs module. \
            The main goal is to serve as a tool to create and modify files in the file system. \
            The program outputs reads to any requested path to the stdin.")
        .subcommand(
            SubCommand::with_name("ls")
                .about("List the given directory")
                .arg(Arg::with_name("PATH").index(1).required(true))
        )
        .subcommand(
            SubCommand::with_name("cat")
                .about("Ouput the given file to the stdout")
                .arg(Arg::with_name("PATH").index(1).required(true))
        )
        .subcommand(
            SubCommand::with_name("mkdir")
                .about("Create a directory named by the given path with the given label")
                .arg(Arg::with_name("PATH").index(1).required(true))
                .arg(Arg::with_name("secrecy")
                    .short("s")
                    .long("secrecy")
                    .multiple(true)
                    .value_delimiter(";")
                    .require_delimiter(true)
                    .value_name("SECRECY CLAUSE")
                    .required(true)
                    .help("A DCLabel clause is a string of comma-delimited principals. Multiple clauses must be delimited by semi-colons."))
                .arg(Arg::with_name("integrity")
                    .short("i")
                    .long("integrity")
                    .multiple(true)
                    .value_delimiter(";")
                    .require_delimiter(true)
                    .value_name("INTEGRITY CLAUSE")
                    .required(true)
                    .help("A DCLabel clause is a string of comma-delimited principals. Multiple clauses must be delimited by semi-colons."))
                .arg(Arg::with_name("endorse")
                    .short("e")
                    .long("endorse")
                    .required(true)
                    .takes_value(true)
                    .help("Endorse the creation with the given principal"))
        )
        .subcommand(
            SubCommand::with_name("mkfile")
                .about("Create a file named by the given path with the given label")
                .arg(Arg::with_name("PATH").index(1).required(true))
                .arg(Arg::with_name("secrecy")
                    .short("s")
                    .long("secrecy")
                    .multiple(true)
                    .value_delimiter(";")
                    .require_delimiter(true)
                    .value_name("SECRECY CLAUSE")
                    .required(true)
                    .help("A DCLabel clause is a string of comma-delimited principals. Multiple clauses must be delimited by semi-colons."))
                .arg(Arg::with_name("integrity")
                    .short("i")
                    .long("integrity")
                    .multiple(true)
                    .value_delimiter(";")
                    .require_delimiter(true)
                    .value_name("INTEGRITY CLAUSE")
                    .required(true)
                    .help("A DCLabel clause is a string of comma-delimited principals. Multiple clauses must be delimited by semi-colons."))
                .arg(Arg::with_name("endorse")
                    .short("e")
                    .long("endorse")
                    .required(true)
                    .takes_value(true)
                    .help("Endorse the creation with the given principal"))
        )
        .subcommand(
            SubCommand::with_name("write")
                .about("Overwrite the given file with the data from the given file or the stdin")
                .arg(Arg::with_name("PATH").index(1).required(true))
                .arg(Arg::with_name("FILE")
                    .short("f")
                    .long("file")
                    .takes_value(true)
                    .value_name("FILE"))
                .arg(Arg::with_name("endorse")
                    .short("e")
                    .long("endorse")
                    .required(true)
                    .takes_value(true)
                    .help("Endorse the modification with the given principal"))
        )
        .get_matches();

    let mut cur_label = DCLabel::public();
    match cmd_arguments.subcommand() {
        ("cat", Some(sub_m)) => {
            if let Ok(data) = labeled_fs::read(sub_m.value_of("PATH").unwrap(), &mut cur_label) {
                std::io::stdout().write_all(&data).unwrap();
            } else {
                eprintln!("Invalid path.");
            }
        },
        ("ls", Some(sub_m)) => {
            if let Ok(list) = labeled_fs::list(sub_m.value_of("PATH").unwrap(), &mut cur_label) {
                let output = list.join("\t");
                println!("{}", output);
            } else {
                eprintln!("Invalid path.");
            }
        },
        ("mkdir", Some(sub_m)) => {
            let path = std::path::Path::new(sub_m.value_of("PATH").unwrap());
            let s_clauses: Vec<&str> = sub_m.values_of("secrecy").unwrap().collect();
            let i_clauses: Vec<&str> = sub_m.values_of("integrity").unwrap().collect();
            cur_label = input_to_endorsement(sub_m.value_of("endorse").unwrap());
            match labeled_fs::create_dir(
                path.parent().unwrap().to_str().unwrap(),
                path.file_name().unwrap().to_str().unwrap(),
                input_to_dclabel([s_clauses, i_clauses]),
                &mut cur_label) {
                Err(labeled_fs::Error::BadPath) => {
                    eprintln!("Invalid path.");
                },
                Err(labeled_fs::Error::Unauthorized) => {
                    eprintln!("Bad endorsement.");
                },
                Err(labeled_fs::Error::BadTargetLabel) => {
                    eprintln!("Bad target label.");
                },
                Ok(()) => {},
            }
        },
        ("mkfile", Some(sub_m)) => {
            let path = std::path::Path::new(sub_m.value_of("PATH").unwrap());
            let s_clauses: Vec<&str> = sub_m.values_of("secrecy").unwrap().collect();
            let i_clauses: Vec<&str> = sub_m.values_of("integrity").unwrap().collect();
            cur_label = input_to_endorsement(sub_m.value_of("endorse").unwrap());
            match labeled_fs::create_file(
                path.parent().unwrap().to_str().unwrap(),
                path.file_name().unwrap().to_str().unwrap(),
                input_to_dclabel([s_clauses, i_clauses]),
                &mut cur_label) {
                Err(labeled_fs::Error::BadPath) => {
                    eprintln!("Invalid path.");
                },
                Err(labeled_fs::Error::Unauthorized) => {
                    eprintln!("Bad endorsement.");
                },
                Err(labeled_fs::Error::BadTargetLabel) => {
                    eprintln!("Bad target label.");
                },
                Ok(()) => {},
            }
        },
        ("write", Some(sub_m)) => {
            let data = sub_m.value_of("FILE").map_or_else(
                || {
                    let mut buf = Vec::new();
                    std::io::stdin().read_to_end(&mut buf).unwrap();
                    buf
                },
                |p| std::fs::read(p).unwrap()
            );
            cur_label = input_to_endorsement(sub_m.value_of("endorse").unwrap());
            match labeled_fs::write(sub_m.value_of("PATH").unwrap(), data, &mut cur_label) {
                Err(labeled_fs::Error::BadPath) => {
                    eprintln!("Invalid path.");
                },
                Err(labeled_fs::Error::Unauthorized) => {
                    eprintln!("Bad endorsement.");
                },
                Err(labeled_fs::Error::BadTargetLabel) => {
                    eprintln!("write should not reach here.");
                },
                Ok(()) => {},
            }
        },
        (&_, _) => {
            eprintln!("{}", cmd_arguments.usage());
        }
    }
}
