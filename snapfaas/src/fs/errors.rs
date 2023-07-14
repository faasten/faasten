#[derive(Debug)]
pub enum FsError {
    BadPath,
    NotADir,
    NotAFacetedDir,
    NotAFile,
    NotABlob,
    NotAGate,
    NotAService,
    MalformedRedirectTarget,
    ClearanceError,
    LabelError(LabelError),
    PrivilegeError(PrivilegeError),
    //FacetedDir(FacetedDirectory, Buckle),
    GateError(GateError),
    LinkError(LinkError),
    UnlinkError(UnlinkError),
    FacetError(FacetError),
    ServiceError(ServiceError),
    NameExists,
    InvalidFd,
}

impl From<LabelError> for FsError {
    fn from(err: LabelError) -> Self {
        FsError::LabelError(err)
    }
}

impl From<PrivilegeError> for FsError {
    fn from(err: PrivilegeError) -> Self {
        FsError::PrivilegeError(err)
    }
}

impl From<GateError> for FsError {
    fn from(err: GateError) -> Self {
        FsError::GateError(err)
    }
}

impl From<LinkError> for FsError {
    fn from(err: LinkError) -> Self {
        FsError::LinkError(err)
    }
}

impl From<UnlinkError> for FsError {
    fn from(err: UnlinkError) -> Self {
        FsError::UnlinkError(err)
    }
}

impl From<FacetError> for FsError {
    fn from(err: FacetError) -> Self {
        FsError::FacetError(err)
    }
}

impl From<ServiceError> for FsError {
    fn from(err: ServiceError) -> Self {
        FsError::ServiceError(err)
    }
}

#[derive(Debug)]
pub enum LabelError {
    CannotRead,
    CannotWrite,
}

#[derive(Debug)]
pub enum PrivilegeError {
    CannotDelegate,
}

#[derive(Debug)]
pub enum LinkError {
    LabelError(LabelError),
    Exists,
}

#[derive(Debug)]
pub enum UnlinkError {
    LabelError(LabelError),
    DoesNotExists,
}

#[derive(Debug)]
pub enum GateError {
    CannotDelegate,
    CannotInvoke,
    Corrupted,
}

#[derive(Debug)]
pub enum FacetError {
    Unallocated,
    LabelError(LabelError),
    NoneValue,
    Corrupted,
}

#[derive(Debug)]
pub enum ServiceError {
    CannotDelegate,
    CannotInvoke,
    Corrupted,
}
