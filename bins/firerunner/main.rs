#[macro_use(crate_version, crate_authors)]
extern crate clap;
extern crate cgroups;

use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::os::unix::io::FromRawFd;
use std::path::{Path, PathBuf};

use clap::{App, Arg};
use snapfaas::vm;

const READY :&[u8] = &[vm::VmStatus::ReadyToReceive as u8];

fn main() {
    let cmd_arguments = App::new("firecracker")
        .version(crate_version!())
        .author(crate_authors!())
        .about("launch a microvm.")
        .arg(
            Arg::with_name("kernel")
                .short("k")
                .long("kernel")
                .value_name("kernel")
                .takes_value(true)
                .required(true)
                .help("path the the kernel binary")
        )
        .arg(
            Arg::with_name("kernel boot args")
                .short("c")
                .long("kernel_args")
                .value_name("kernel_args")
                .takes_value(true)
                .required(false)
                .default_value("quiet console=none reboot=k panic=1 pci=off")
                .help("kernel boot args")
        )
        .arg(
            Arg::with_name("rootfs")
                .short("r")
                .long("rootfs")
                .value_name("rootfs")
                .takes_value(true)
                .required(true)
                .help("path to the root file system")
        )
        .arg(
            Arg::with_name("appfs")
                .long("appfs")
                .value_name("appfs")
                .takes_value(true)
                .required(false)
                .help("path to the root file system")
        )
        .arg(
            Arg::with_name("id")
                .long("id")
                .help("microvm unique identifier")
                .default_value("abcde1234")
                .takes_value(true)
                ,
        )
        .arg(
            Arg::with_name("load_dir")
                .long("load_from")
                .takes_value(true)
                .required(false)
                .help("if specified start vm from a snapshot under the given directory")
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

    // make sure the input arguments are correct when the process is invoked
    let kernel = cmd_arguments.value_of("kernel").unwrap().to_string();
    let rootfs = cmd_arguments.value_of("rootfs").unwrap().to_string();
    let appfs = cmd_arguments.value_of("appfs").unwrap().to_string();
    let kargs = cmd_arguments
        .value_of("kernel boot args")
        .unwrap()
        .to_string();
    let mem_size_mib = cmd_arguments
        .value_of("mem_size")
        .map(|x| x.parse::<usize>().unwrap()).unwrap();
    let vcpu_count = cmd_arguments
        .value_of("vcpu_count")
        .map(|x| x.parse::<u64>().unwrap()).unwrap();
    let load_dir = cmd_arguments.value_of("load_dir").map(PathBuf::from);
    let dump_dir = cmd_arguments.value_of("dump_dir").map(PathBuf::from);

    // It's safe to unwrap here because clap's been provided with a default value
    let instance_id = cmd_arguments.value_of("id").unwrap().to_string();

    // output file for debugging
    let file_name = format!("out/vm-{}.log", instance_id);
    let output_file = File::create(file_name);
    if let Err(e) = output_file {
        panic!("Cannot create output file, {:?}", e);
    }

    let mut output_file = output_file.unwrap();
    write!(&mut output_file, "vm-{:?}\n", instance_id);
    write!(&mut output_file, "appfs: {:?}\n", appfs);
    write!(&mut output_file, "rootfs: {:?}\n", rootfs);
    write!(&mut output_file, "memory size: {:?}\n", mem_size_mib);
    write!(&mut output_file, "vcpu count: {:?}\n", vcpu_count);
    write!(&mut output_file, "load dir: {:?}\n", load_dir);
    write!(&mut output_file, "dump_dir: {:?}\n", dump_dir);
    output_file.flush();

    // wait time in ms
    let wait_time  = match appfs.as_ref() {
        "lorempy2.ext4" => 20,
        "loremjs.ext4" => 20,
        "markdown-to-html.ext4" => 600,
        "img-resize.ext4" => 1800,
        "ocr-img.ext4" => 30000,
        "sentiment-analysis.ext4" => 5000,
        "autocomplete.ext4" => 100,
        _ => 0,
    };

    // notify snapfaas that the vm is ready
    std::thread::sleep(std::time::Duration::from_millis(300));
    io::stdout().write_all(READY);
    io::stdout().flush();
    
    println!("here");
    let mut req_count = 0;
    // continuously read from stdio
    loop {
        let mut req_header = vec![0;1];
        let mut stdin = io::stdin();
        // does this block when there's nothing in stdin?
        // first read in the number of bytes that I should read from stdin
        stdin.lock().read(&mut req_header);
        let size = req_header[0] as usize;

        // actually read the request
        let mut req_buf = vec![0;size];
        stdin.lock().read_exact(&mut req_buf);

        // sleep to simulate running
        std::thread::sleep(std::time::Duration::from_millis(wait_time as u64));
        write!(&mut output_file, "process time: {}\n", wait_time);

        //stdout.lock().write_fmt(format_args!("success"));
        // first write the number bytes in the response to stdout
        io::stdout().write_all(&[size as u8]);
        io::stdout().flush();
        // write the actual response
        io::stdout().write_all(&req_buf);
        io::stdout().flush();

        req_count = req_count+1;
        write!(&mut output_file, "Done: {}\n", req_count);
        output_file.flush();
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
