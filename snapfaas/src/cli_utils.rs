// Functions in this file converts formatted strings to DCLabels.
// They are now used by bins/sffs.
// A clause string is comma-joined principal strings, or "true", or "false".
// A secrecy or a integrity string is either "true", "false", or semi-colon-joined clause strings
// A endorsement string is expected to be a principal string.
use labeled::dclabel::{self, DCLabel};

use std::collections::BTreeSet;

use crate::syscalls::PathComponent;
use crate::syscalls::path_component::Component::{Facet, Name};

pub fn input_to_dclabel(si_clauses: [Vec<&str>; 2]) -> DCLabel {
    let mut components = Vec::new();
    for clauses in si_clauses {
        let component: dclabel::Component = if clauses[0].to_lowercase() == "true" {
            true.into()
        } else if clauses[0].to_lowercase() == "false" {
            false.into()
        } else {
            let mut s = BTreeSet::new();
            for clause in clauses {
                let c: BTreeSet<String> = clause.split(",").map(|s| s.to_lowercase()).collect();
                s.insert(dclabel::Clause::from(c));
            }
            dclabel::Component::from(s)
        };
        components.push(component);
    }
    let secrecy = components.remove(0);
    let integrity = components.remove(0);
    DCLabel::new(secrecy, integrity)
}

pub fn input_to_endorsement(endorse: &str) -> DCLabel {
    if endorse.to_lowercase() == "false" {
        DCLabel::new(true, false)
    } else if endorse.to_lowercase() == "true" {
        DCLabel::new(true, true)
    } else {
        DCLabel::new(true, [[endorse.to_lowercase()]])
    }
}

pub fn input_to_path(input: &str) -> Vec<PathComponent> {
    input.strip_prefix('/').expect("Path must start with /")
        .split('/').map(|s| {
            if s.starts_with('[') {
                let label = s.strip_prefix('[').unwrap();
                let label = label.strip_suffix(']').expect("Malformed label: missing ]");
                let si_clauses: Vec<&str> = label.split('#').collect();
                let s_clauses = si_clauses.get(0).expect("Malformed label: missing #")
                    .split(';').collect::<Vec<&str>>();
                let i_clauses = si_clauses.get(1).expect("Malformed label: missing #")
                    .split(';').collect::<Vec<&str>>();
                let label = input_to_dclabel([s_clauses, i_clauses]);
                PathComponent { component: Some(Facet(crate::vm::dc_label_to_proto_label(&label))) }
            } else {
                PathComponent { component: Some(Name(s.to_string())) }
            }
        }).collect()
}
