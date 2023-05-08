fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::compile_protos(&["src/syscalls.proto", "src/sched/messages.proto"], &["src/"])?;
    capnpc::CompilerCommand::new().file("src/syscalls.capnp").run().unwrap();
    Ok(())
}
