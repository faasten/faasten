pub mod syscalls_capnp {
    include!(concat!(env!("OUT_DIR"), "/src/syscalls_capnp.rs"));
}
include!(concat!(env!("OUT_DIR"), "/snapfaas.syscalls.rs"));
