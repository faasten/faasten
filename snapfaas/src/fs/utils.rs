//! Util functions called by admin_fstools
use super::*;
use labeled::buckle::{Buckle, Component};
use labeled::Label;

pub fn create_or_update_file<S: BackingStore, P: Into<self::path::Path>>(
    fs: &FS<S>,
    base_dir: P,
    name: String,
    label: Buckle,
    data: Vec<u8>,
) -> Result<(), FsError> {
    if let DirEntry::Directory(dir) = fs.read_path(base_dir)? {
        match dir.list(fs).get(&name) {
            Some(DirEntry::File(fileentry)) => fileentry.write(data, fs).map_err(Into::into),
            Some(_) => {
                dir.unlink(&name, fs)?;
                let new_file = fs.create_file(label);
                match new_file {
                    DirEntry::File(filentry) => {
                        filentry.write(data, fs).map_err(Into::<FsError>::into)?
                    }
                    _ => panic!("should never reach here."),
                }
                dir.link(name, new_file, fs)?;
                Ok(())
            }
            None => {
                let new_file = fs.create_file(label);
                match new_file {
                    DirEntry::File(filentry) => {
                        filentry.write(data, fs).map_err(Into::<FsError>::into)?
                    }
                    _ => panic!("should never reach here."),
                }
                dir.link(name, new_file, fs)?;
                Ok(())
            }
        }
    } else {
        Err(FsError::BadPath)
    }
}

pub fn create_or_update_blob<S: BackingStore, P: Into<self::path::Path>>(
    fs: &FS<S>,
    base_dir: P,
    name: String,
    label: Buckle,
    blob_name: String,
) -> Result<(), FsError> {
    if let DirEntry::Directory(dir) = fs.read_path(base_dir)? {
        match dir.list(fs).get(&name) {
            Some(DirEntry::Blob(blobentry)) => blobentry.replace(blob_name, fs).map_err(Into::into),
            Some(_) => {
                dir.unlink(&name, fs)?;
                let new_blob = fs.create_blob(label, blob_name)?;
                dir.link(name, new_blob, fs)?;
                Ok(())
            }
            None => {
                let new_blob = fs.create_blob(label, blob_name)?;
                dir.link(name, new_blob, fs)?;
                Ok(())
            }
        }
    } else {
        Err(FsError::BadPath)
    }
}

pub fn create_faceted<S: BackingStore, P: Into<self::path::Path>>(
    fs: &FS<S>,
    base_dir: P,
    name: String,
) -> Result<(), FsError> {
    let base_dir = base_dir.into();
    if !fs.list_dir(base_dir.clone())?.contains_key(&name) {
        let new_dir = fs.create_faceted_directory();
        fs.link(base_dir, name, new_dir)
    } else {
        Err(FsError::NameExists)
    }
}

pub fn resolve_gate_with_clearance_check<S: BackingStore, P: Into<self::path::Path>>(
    fs: &FS<S>,
    path: P,
) -> Result<(Function, Component), FsError> {
    match fs.read_path(path)? {
        DirEntry::Gate(gate) => {
            let direct_gate = gate.to_invokable(fs);
            PRIVILEGE.with(|p| {
                let privilege = p.borrow();
                if privilege.implies(&direct_gate.invoker_integrity_clearance) {
                    Ok((direct_gate.function, direct_gate.privilege))
                } else {
                    Err(FsError::GateError(GateError::CannotInvoke))
                }
            })
        }
        _ => Err(FsError::NotAGate),
    }
}

// BEGIN LABEL UTILS (should be outside fs module)

pub fn get_current_label() -> Buckle {
    let res = CURRENT_LABEL.with(|l| l.borrow().clone());
    res
}

pub fn get_privilege() -> Component {
    let res = PRIVILEGE.with(|l| l.borrow().clone());
    res
}

pub fn get_ufacet() -> Buckle {
    let res = PRIVILEGE.with(|p| Buckle {
        secrecy: p.borrow().clone(),
        integrity: p.borrow().clone(),
    });
    res
}

pub fn taint_with_label(label: Buckle) -> Buckle {
    let res = CURRENT_LABEL.with(|l| {
        let clone = l.borrow().clone();
        *l.borrow_mut() = clone.lub(label);
        l.borrow().clone()
    });
    res
}

pub fn clear_label() {
    CURRENT_LABEL.with(|current_label| {
        *current_label.borrow_mut() = Buckle::public();
    });
}

pub fn set_my_privilge(newpriv: Component) {
    PRIVILEGE.with(|opriv| {
        *opriv.borrow_mut() = newpriv;
    });
}

pub fn declassify_with(my_priv: &<Buckle as HasPrivilege>::Privilege) -> Buckle {
    CURRENT_LABEL.with(|l| {
        let my_label = get_current_label();
        let mut current_label = l.borrow_mut();
        *current_label = my_label.downgrade(my_priv);
        current_label.clone()
    })
}

pub fn declassify(target: Component) -> Result<Buckle, Buckle> {
    let res = CURRENT_LABEL.with(|l| {
        PRIVILEGE.with(|opriv| {
            if (target.clone() & opriv.borrow().clone()).implies(&l.borrow().secrecy) {
                Ok(Buckle::new(target, l.borrow().integrity.clone()))
            } else {
                Err(l.borrow().clone())
            }
        })
    });
    res
}
