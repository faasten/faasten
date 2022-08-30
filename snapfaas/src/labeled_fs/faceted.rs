use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use labeled::dclabel::DCLabel;
use labeled::Label;

use super::{LabeledDirEntry, DirEntry, Error, Directory, get_uid, put_val_db_no_overwrite};

type Result<T> = std::result::Result::<T, Error>;

/// We implement a faceted directory as a mapping from SerializedDCLabel (String) to LabeledDirEntry
/// to make a faceted directory behave like a regular directory during the path traversal.
/// However, faceted directories do not implement `create` and `list` because all facets virtually
/// exists.
#[derive(Serialize, Deserialize)]
pub struct FacetedDirectory {
    // ordered by secrecy, then by integrity, under each label a mapping from names to direntries
    facets: HashMap<String, LabeledDirEntry>,
}

impl FacetedDirectory {
    pub fn new() -> Self {
        Self { facets: HashMap::new() }
    }

    pub fn from_vec(buf: Vec<u8>) -> Self {
        serde_json::from_slice(&buf).unwrap()
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    pub fn get(&self, facet: &str) -> Result<&LabeledDirEntry> {
        self.facets.get(facet).ok_or(Error::BadPath)
    }

    pub fn allocate(
        &mut self,
        facet: &str,
        label: DCLabel,
        cur_label: &DCLabel,
        txn: &mut lmdb::RwTransaction,
        db: lmdb::Database
    ) -> Result<&LabeledDirEntry> {
        if cur_label.can_flow_to(&label) {
            let mut uid = get_uid();
            while put_val_db_no_overwrite(uid, Directory::new().to_vec(), txn, db).is_err() {
                uid = super::get_uid();
            }
            let new_entry = LabeledDirEntry::new(label, DirEntry::D, uid);
            let _ = self.facets.insert(facet.to_string(), new_entry);
            Ok(self.facets.get(facet).unwrap())
        } else {
            Err(Error::Unauthorized)
        }
    }
}
