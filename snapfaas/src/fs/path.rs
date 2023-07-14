use std::collections::VecDeque;

use labeled::buckle::Buckle;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum Error {
    InvalidName,
    InvalidFacet,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum PathComponent {
    Dscrp(String),
    Facet(Buckle),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Path {
    components: VecDeque<PathComponent>,
}

impl Path {
    // FIXME need a proper parser
    /// The root should be written as ":" in string, though the function below
    /// does not enforce it.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let input = input.trim_matches(':');
        let mut components = VecDeque::new();
        let mut cs = input.split(":").peekable();
        match cs.peek().unwrap() {
            &"" => return Ok(Path { components }),
            &"~" => {
                components.push_back(PathComponent::Dscrp("home".to_string()));
                components.push_back(PathComponent::Facet(super::utils::get_ufacet()));
                let _ = cs.next();
            }
            &_ => (),
        }

        let label_re = regex::Regex::new(r"^<(?P<lbl>.+)>$").unwrap();
        for c in cs {
            if c == "%" {
                components.push_back(PathComponent::Facet(super::utils::get_current_label()));
            } else if label_re.is_match(c) {
                let lblstr = label_re
                    .captures(c)
                    .and_then(|cap| cap.name("lbl").map(|lbl| lbl.as_str()))
                    .ok_or(Error::InvalidFacet)?;
                let f = Buckle::parse(lblstr).map_err(|_| Error::InvalidFacet)?;
                components.push_back(PathComponent::Facet(f));
            } else {
                components.push_back(PathComponent::Dscrp(c.to_string()));
            }
        }
        Ok(Self { components })
    }

    /// The root is represented as an empty vector of path::Component's
    pub fn root() -> Self {
        Self { components: VecDeque::new() }
    }

    pub fn pop_front(&mut self) -> Option<PathComponent> {
        self.components.pop_front()
    }

    pub fn parent(&self) -> Option<Self> {
        let mut res = self.clone();
        res.components.pop_back();
        if res.components.is_empty() {
            None
        } else {
            Some(res)
        }
    }

    pub fn file_name(&self) -> Option<String> {
        self.components.back().and_then(|last|
            match last {
                PathComponent::Dscrp(s) => Some(s.clone()),
                PathComponent::Facet(_) => None,
            }
        )
    }

    pub fn push_dscrp(&mut self, s: String) {
        self.components.push_back(PathComponent::Dscrp(s));
    }
}

impl IntoIterator for Path {
    type Item = PathComponent;
    type IntoIter = std::collections::vec_deque::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.components.into_iter()
    }
}

impl<'a> IntoIterator for &'a Path {
    type Item = &'a PathComponent;
    type IntoIter = std::collections::vec_deque::Iter<'a, PathComponent>;

    fn into_iter(self) -> Self::IntoIter {
        self.components.iter()
    }
}

/*impl From<Vec<syscalls::PathComponent>> for Path {
    fn from(p: Vec<syscalls::PathComponent>) -> Self {
        let components = p.into_iter().fold(VecDeque::new(), |mut acc, c| {
            acc.push_back(match c.component.unwrap() {
                syscalls::path_component::Component::Dscrp(s) => PathComponent::Dscrp(s),
                syscalls::path_component::Component::Facet(f) => {
                    PathComponent::Facet(pblabel_to_buckle(&f))
                }
            });
            acc
        });
        Self { components }
    }
}*/
