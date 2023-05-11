use std::collections::BTreeSet;

use capnp::{capability::Promise, Error};
use labeled::buckle::Principal;

pub mod syscalls_capnp {
    include!(concat!(env!("OUT_DIR"), "/src/syscalls_capnp.rs"));
}
include!(concat!(env!("OUT_DIR"), "/snapfaas.syscalls.rs"));

pub fn buckle_to_capnp_label(input: &labeled::buckle::Buckle, output: &mut syscalls_capnp::buckle::Builder) {
    use labeled::buckle::Component;
    match input.secrecy {
        Component::DCFalse => {
            output.reborrow().init_secrecy().set_false(());
        },
        Component::DCFormula(ref clauses) => {
            let mut component = output.reborrow().init_secrecy().init_component(clauses.len() as u32);
            for (i, c) in clauses.iter().enumerate() {
                let mut clause = component.reborrow().init(i as u32, c.0.len() as u32);
                for (j, p) in c.0.iter().enumerate() {
                    let mut principle = clause.reborrow().init(j as u32, p.len() as u32);
                    for (k, s) in p.iter().enumerate() {
                        principle.reborrow().set(k as u32, s);
                    }
                }
            }
        }
    }

    match input.integrity {
        Component::DCFalse => {
            output.reborrow().init_integrity().set_false(());
        },
        Component::DCFormula(ref clauses) => {
            let mut component = output.reborrow().init_integrity().init_component(clauses.len() as u32);
            for (i, c) in clauses.iter().enumerate() {
                let mut clause = component.reborrow().init(i as u32, c.0.len() as u32);
                for (j, p) in c.0.iter().enumerate() {
                    let mut principle = clause.reborrow().init(j as u32, p.len() as u32);
                    for (k, s) in p.iter().enumerate() {
                        principle.reborrow().set(k as u32, s);
                    }
                }
            }
        }
    }
}

pub fn buckle_from_capnp_label(input: &syscalls_capnp::buckle::Reader) -> Result<labeled::buckle::Buckle, Error> {
    let secrecy = match input.get_secrecy()?.which()? {
        syscalls_capnp::buckle::component::Which::False(()) => {
            labeled::buckle::Component::DCFalse
        },
        syscalls_capnp::buckle::component::Which::Component(component) => {
            let mut component_set = BTreeSet::<labeled::buckle::Clause>::new();
            for clause in component?.into_iter() {
                let mut clause_set: BTreeSet<Vec<Principal>> = BTreeSet::new();
                for princ in clause?.into_iter() {
                    let p: Vec<Principal> = princ?.iter().map(|x| String::from(x.unwrap())).collect();
                    clause_set.insert(p);
                }
                component_set.insert(labeled::buckle::Clause(clause_set));
            }
            //labeled::buckle::Component::formula([])
            labeled::buckle::Component::DCFormula(component_set)
        }
    };
    let integrity = match input.get_integrity()?.which()? {
        syscalls_capnp::buckle::component::Which::False(()) => {
            labeled::buckle::Component::DCFalse
        },
        syscalls_capnp::buckle::component::Which::Component(component) => {
            let mut component_set = BTreeSet::<labeled::buckle::Clause>::new();
            for clause in component?.into_iter() {
                let mut clause_set: BTreeSet<Vec<Principal>> = BTreeSet::new();
                for princ in clause?.into_iter() {
                    let p: Vec<Principal> = princ?.iter().map(|x| String::from(x.unwrap())).collect();
                    clause_set.insert(p);
                }
                component_set.insert(labeled::buckle::Clause(clause_set));
            }
            //labeled::buckle::Component::formula([])
            labeled::buckle::Component::DCFormula(component_set)
        }
    };
    Ok(labeled::buckle::Buckle::new(secrecy, integrity))
}
