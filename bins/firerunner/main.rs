#[macro_use(crate_version, crate_authors)]
extern crate clap;
extern crate cgroups;

use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::os::unix::io::FromRawFd;
use std::path::PathBuf;

use clap::{App, Arg};
use snapfaas::vm;

const READY :&[u8] = &[vm::VmStatus::ReadyToReceive as u8];
fn main() {
    let cmd_arguments = App::new("firecracker")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Launch a microvm.")
        .arg(
            Arg::with_name("kernel")
                .short("k")
                .long("kernel")
                .value_name("KERNEL")
                .takes_value(true)
                .required(true)
                .help("Path the the kernel binary")
        )
        .arg(
            Arg::with_name("kernel boot args")
                .short("c")
                .long("kernel_args")
                .value_name("KERNEL_ARGS")
                .takes_value(true)
                .required(false)
                .default_value("quiet console=none reboot=k panic=1 pci=off")
                .help("Kernel boot args")
        )
        .arg(
            Arg::with_name("rootfs")
                .long("r")
                .long("rootfs")
                .value_name("ROOTFS")
                .takes_value(true)
                .required(true)
                .help("Path to the root file system")
        )
        .arg(
            Arg::with_name("appfs")
                .long("appfs")
                .value_name("APPFS")
                .takes_value(true)
                .required(false)
                .help("Path to the root file system")
        )
        .arg(
            Arg::with_name("id")
                .long("id")
                .help("MicroVM unique identifier")
                .default_value("abcde1234")
                .takes_value(true)
                ,
        )
        .arg(
            Arg::with_name("seccomp-level")
                .long("seccomp-level")
                .help(
                    "Level of seccomp filtering.\n
                            - Level 0: No filtering.\n
                            - Level 1: Seccomp filtering by syscall number.\n
                            - Level 2: Seccomp filtering by syscall number and argument values.\n
                        ",
                )
                .takes_value(true)
                .default_value("0")
                .possible_values(&["0", "1", "2"]),
        )
        .arg(
            Arg::with_name("load_dir")
                .long("load_from")
                .takes_value(true)
                .required(false)
                .help("if specified start VM from a snapshot under the given directory")
        )
        .arg(
            Arg::with_name("dump_dir")
                .long("dump_to")
                .takes_value(true)
                .required(false)
                .help("if specified creates a snapshot right after runtime is up under the given directory")
        )
        .arg(
            Arg::with_name("mem_size")
                 .long("mem_size")
                 .value_name("MEMSIZE")
                 .takes_value(true)
                 .required(true)
                 .help("Guest memory size in MB (default is 128)")
        )
        .arg(
            Arg::with_name("vcpu_count")
                 .long("vcpu_count")
                 .value_name("VCPUCOUNT")
                 .takes_value(true)
                 .required(true)
                 .help("Number of vcpus (default is 1)")
        )
        .get_matches();

    let kernel = cmd_arguments.value_of("kernel").unwrap().to_string();
    // let rootfs = [cmd_arguments.value_of("rootfs").unwrap()].iter().collect();
    // let appfs = cmd_arguments.value_of("appfs").map(|s| [s].iter().collect());
    let rootfs = cmd_arguments.value_of("rootfs").unwrap().to_string();
    let appfs = cmd_arguments.value_of("appfs").unwrap().to_string();
    let kargs = cmd_arguments
        .value_of("kernel boot args")
        .unwrap()
        .to_string();
    let mem_size_mib = cmd_arguments
        .value_of("mem_size")
        .map(|x| x.parse::<usize>().unwrap());
    let vcpu_count = cmd_arguments
        .value_of("vcpu_count")
        .map(|x| x.parse::<u64>().unwrap());
    let load_dir = cmd_arguments.value_of("load_dir").map(PathBuf::from);
    let dump_dir = cmd_arguments.value_of("dump_dir").map(PathBuf::from);

    // It's safe to unwrap here because clap's been provided with a default value
    let instance_id = cmd_arguments.value_of("id").unwrap().to_string();

    // We disable seccomp filtering when testing, because when running the test_gnutests
    // integration test from test_unittests.py, an invalid syscall is issued, and we crash
    // otherwise.
    #[cfg(test)]
    let seccomp_level = seccomp::SECCOMP_LEVEL_NONE;
    #[cfg(not(test))]
    // It's safe to unwrap here because clap's been provided with a default value,
    // and allowed values are guaranteed to parse to u32.
    let seccomp_level = cmd_arguments
        .value_of("seccomp-level")
        .unwrap()
        .parse::<u32>()
        .unwrap();

    io::stdout().write_all(READY);
    io::stdout().flush();
    
    loop {
        let mut stdin = io::stdin();
        let mut req_buf = vec![0;64];
        stdin.lock().read(&mut req_buf);

        //stdout.lock().write_fmt(format_args!("success"));
        io::stdout().write_all(&req_buf);
        io::stdout().flush();
        //stdout.lock().write_fmt(format_args!("echo: {:?}", req_buf));
    }

    /*
    let (checker, notifier) = nix::unistd::pipe().expect("Could not create a pipe");

    let mut app = VmAppConfig {
        kernel,
        instance_id,
        rootfs,
        appfs,
        cmd_line,
        seccomp_level,
        vsock_cid: 42,
        notifier: unsafe{ File::from_raw_fd(notifier) },
        cpu_share: 1024,
        vcpu_count: vcpu_count.unwrap_or(1),
        mem_size_mib,
        load_dir,
        dump_dir,
    }.run(true, None);

    // We need to wait for the ready signal from Firecracker
    let data = &mut[0u8; 4usize];
    unsafe{ File::from_raw_fd(checker) }.read_exact(data).expect("Failed to receive ready signal");
    println!("VM with notifier id {} is ready", u32::from_le_bytes(*data));

    let stdin = std::io::stdin();

    for mut line in stdin.lock().lines().map(|l| l.unwrap()) {
        line.push('\n');
        app.connection.write_all(line.as_bytes()).expect("Failed to write to request pipe");
        let mut lens = [0; 4];
        app.connection.read_exact(&mut lens).expect("Failed to read response size");
        let len = u32::from_be_bytes(lens);
        let mut response = vec![0; len as usize];
        app.connection.read_exact(response.as_mut_slice()).expect("Failed to read response");
        println!("{}", String::from_utf8(response).unwrap());
    }
    app.kill();
    */
}
