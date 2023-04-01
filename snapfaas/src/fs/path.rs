use labeled::buckle::Buckle;
use serde::{Deserialize, Serialize};

use crate::syscall_server::pblabel_to_buckle;
use crate::syscalls;

#[derive(Debug)]
pub enum Error {
    InvalidName,
    InvalidFacet,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Component {
    Dscrp(String),
    Facet(Buckle),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Path {
    components: Vec<Component>,
}

impl Path {
    // FIXME need a proper parser
    /// The root should be written as ":" in string, though the function below
    /// does not enforce it.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let input = input.trim_matches(':');
        let mut components = vec![];
        let mut cs = input.split(":").peekable();
        match cs.peek().unwrap() {
            &"" => return Ok(Path { components }),
            &"~" => {
                components.append(&mut vec![
                    Component::Dscrp("home".to_string()),
                    Component::Facet(super::utils::get_ufacet()),
                ]);
                let _ = cs.next();
            }
            &_ => (),
        }

        let filename_re = regex::Regex::new(r"^([[:word:]]+\.)*[[:word:]]+$").unwrap();
        let label_re = regex::Regex::new(r"^<(?P<lbl>.+)>$").unwrap();
        for c in cs {
            if c == "%" {
                components.push(Component::Facet(super::utils::get_current_label()));
            } else if label_re.is_match(c) {
                let lblstr = label_re
                    .captures(c)
                    .and_then(|cap| cap.name("lbl").map(|lbl| lbl.as_str()))
                    .ok_or(Error::InvalidFacet)?;
                let f = Buckle::parse(lblstr).map_err(|_| Error::InvalidFacet)?;
                components.push(Component::Facet(f));
            } else {
                if filename_re.is_match(c) {
                    components.push(Component::Dscrp(c.to_string()));
                } else {
                    return Err(Error::InvalidName);
                }
            }
        }
        Ok(Self { components })
    }

    /// The root is represented as an empty vector of path::Component's
    pub fn root() -> Self {
        Self { components: vec![] }
    }

    pub fn split_last(&self) -> Option<(&Component, &[Component])> {
        self.components.split_last()
    }

    pub fn parent(&self) -> Option<Self> {
        self.split_last().map(|(_, prefix)| Self {
            components: Vec::from(prefix),
        })
    }

    pub fn file_name(&self) -> Option<String> {
        self.split_last().and_then(|(last, _)| match last {
            Component::Dscrp(s) => Some(s.clone()),
            Component::Facet(_) => None,
        })
    }

    pub fn push_dscrp(&mut self, s: String) {
        self.components.push(Component::Dscrp(s));
    }

    //pub fn from_pb(input: &Vec<syscalls::PathComponent>) -> Self {
    //
    //}

    // pair("~:", separated_list(alnum_string|label_string, tag(":"))
    // or
    // separated_list(alnum_string|label_string, tag(":"))
    //fn parser(input: &str) {
    //    use nom::{
    //        combinator::opt,
    //        bytes::complete::tag,
    //        character::complete::alphanumeric1,
    //        multi::separated_list1,
    //    };
    //}
}

impl IntoIterator for Path {
    type Item = Component;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.components.into_iter()
    }
}

impl<'a> IntoIterator for &'a Path {
    type Item = &'a Component;
    type IntoIter = std::slice::Iter<'a, Component>;

    fn into_iter(self) -> Self::IntoIter {
        self.components.iter()
    }
}

impl From<Vec<syscalls::PathComponent>> for Path {
    fn from(p: Vec<syscalls::PathComponent>) -> Self {
        let components = p.into_iter().fold(Vec::new(), |mut acc, c| {
            acc.push(match c.component.unwrap() {
                syscalls::path_component::Component::Dscrp(s) => Component::Dscrp(s),
                syscalls::path_component::Component::Facet(f) => {
                    Component::Facet(pblabel_to_buckle(&f))
                }
            });
            acc
        });
        Self { components }
    }
}
