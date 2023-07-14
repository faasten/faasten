include!(concat!(env!("OUT_DIR"), "/snapfaas.syscalls.rs"));

impl Into<labeled::buckle::Component> for Component {
    fn into(self) -> labeled::buckle::Component {
        match self.component.unwrap() {
            component::Component::DcFalse(_) => labeled::buckle::Component::DCFalse,
            component::Component::Clauses(list) => labeled::buckle::Component::DCFormula(
                list.clauses
                    .iter()
                    .map(|c| {
                        labeled::buckle::Clause(
                            c.principals
                                .iter()
                                .map(|p| p.tokens.iter().cloned().collect())
                                .collect(),
                        )
                    })
                    .collect(),
            ),
        }
    }
}

impl From<labeled::buckle::Component> for Component {
    fn from(value: labeled::buckle::Component) -> Self {
        match value {
            labeled::buckle::Component::DCFalse => Component { component: Some(component::Component::DcFalse(Void {})) },
            labeled::buckle::Component::DCFormula(set) => Component { component: Some(component::Component::Clauses(ClauseList {
                clauses: set
                    .iter()
                    .map(|clause| Clause {
                        principals: clause
                            .0
                            .iter()
                            .map(|vp| TokenList { tokens: vp.clone() })
                            .collect(),
                    })
                    .collect(),
                }))},
        }
    }
}

impl Into<labeled::buckle::Buckle> for Buckle {
    fn into(self) -> labeled::buckle::Buckle {
        labeled::buckle::Buckle {
            secrecy: self.secrecy.unwrap().into(),
            integrity: self.integrity.unwrap().into(),
        }
    }
}

impl From<labeled::buckle::Buckle> for Buckle {
    fn from(value: labeled::buckle::Buckle) -> Self {
        Buckle {
            secrecy: Some(value.secrecy.into()),
            integrity: Some(value.integrity.into()),
        }
    }
}

impl From<&crate::fs::DirEntry> for DentKind {
    fn from(value: &crate::fs::DirEntry) -> Self {
        use crate::fs::DirEntry;
        match value {
            DirEntry::Directory(_) => DentKind::DentDirectory,
            DirEntry::File(_) => DentKind::DentFile,
            DirEntry::FacetedDirectory(_) => DentKind::DentFacetedDirectory,
            DirEntry::Gate(_) => DentKind::DentGate,
            DirEntry::Service(_) => DentKind::DentService,
            DirEntry::Blob(_) => DentKind::DentBlob,
        }
    }
}

impl Into<crate::fs::HttpVerb> for HttpVerb {
    fn into(self) -> crate::fs::HttpVerb {
        match self {
            HttpVerb::HttpHead   => crate::fs::HttpVerb::HEAD,
            HttpVerb::HttpGet    => crate::fs::HttpVerb::GET,
            HttpVerb::HttpPost   => crate::fs::HttpVerb::POST,
            HttpVerb::HttpPut    => crate::fs::HttpVerb::PUT,
            HttpVerb::HttpDelete => crate::fs::HttpVerb::DELETE,
        }
    }
}

impl From<crate::fs::HttpVerb> for HttpVerb {
    fn from(o: crate::fs::HttpVerb) -> Self {
        match o {
            crate::fs::HttpVerb::HEAD   => HttpVerb::HttpHead,
            crate::fs::HttpVerb::GET    => HttpVerb::HttpGet,
            crate::fs::HttpVerb::POST   => HttpVerb::HttpPost,
            crate::fs::HttpVerb::PUT    => HttpVerb::HttpPut,
            crate::fs::HttpVerb::DELETE => HttpVerb::HttpDelete,
        }
    }
}
