use labeled::dclabel::{self, DCLabel};

use std::collections::BTreeSet;

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
