# Machine Setup
## launch docker service

SnapFaaS depends on `docker` so make sure `docker` is installed.
With `docker` installed, start `dockerd`:

```bash
sudo groupadd docker
sudo service docker start
sudo usermod -aG docker $USER
```

The last line gives access permissions to `$USER`
but requires the user to log out the current ssh session and then log back in
to take effect.

## network setup (optional)

Virtual machines use TAP devices as their Ethernet cards. TAP devices must be
connected to a bridge network. We use `docker0` the default bridge device
of `docker`. It should be automatically configured and launched
when the `docker` daemon starts.

To set up NUMBER_OF_TAPS TAP devices, execute:
```bash
scripts/setup-tap-bridge.sh NUMBER_OF_TAPS
```

# Build Binaries
This project is written in Rust. You can install Rust by following instructions [here](https://www.rust-lang.org/tools/install).
The recommended way is to execute the command as below and follow the on-screen instructions:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

This repo contains binaries `multivm`, `singlevm`, and `firerunner`.

To build all binaries, execute
```bash
cargo build
```

To build a specific binary, execute
```bash
cargo build --bin NAME
```

Binaries will be placed in `target/debug` under the project's root directory.
`cargo build --release` will build release version and place binaries in `target/release`.

# Build File Systems

Please refer to [snapfaas-images](https://www.github.com/princeton-sns/snapfaas-images)

# Basic Usage
Use the command below to invoke a Python3 hello world function with requests from `resources/hello.json`
located in the project's root directory.

You need to first build a root file system that contains the Python3 hello world function.
```bash
# run hello function
sudo target/debug/singlevm \
    --kernel resources/vmlinux-4.20.0 \
    --rootfs /path/to/root/file/system/hello-python3.ext4 \
    --mem_size 128 \
    --vcpu_count 1 < resources/hello.json
```
`singlevm` launches a single VM instance and reads in requests from `stdin`.
It is good for testing out new language and new applications.

The command boots a virtual machine with one VCPU and 128MB memory using `resources/vmlinux-4.20.0`
as the uncompressed kernel and a root file system named `hello-python3.ext4` that contains
the Python3 hello world function.
The virtual machine manager will listen on `stdin` for line-delimited json requests and
forward them into the virtual machine. `resources/hello.json` contains example one-line
json requests.

Note: Users should *not* run `firerunner` directly through the command line. Instead,
`multivm` and `singlevm` both fork and run `firerunner` as child processes.

# Cluster Mode
## Quick try-out
To quickly try out `multivm`, run

```bash
scripts/run-multivm-example.sh
```
## More on `multivm`

### the `multivm` binary
The `multivm` binary takes three arguments:

* -\-config|-c /path/to/a/controller/configuration/ymal/file

An example configuration file is at `resources/example-controller-config.yaml`

* -\-mem TOTAL_MEMORY_IN_MB_AVAILABLE_TO_THE_CLUSTER

* -\-listen|-l [ADDR:]PORT

### YAML configuration file
The YAML file specifies the paths to `firerunner` binary, uncompressed kernel, the directory that
stores all root file systems and a list of functions. `multivm` currently only registers functions
statically through the YAML configuration file.

# Working with Snapshots (optional)
## Generate snapshots
Users should use `singlevm` to generate VM snapshots. `singlevm` supports different kinds of snapshots.
All kinds require the use of corresponding file systems.
### language snapshots or fullapp snapshots
```bash
# create the target snapshot directory
mkdir /ssd/snapshots/python3
# generate a Python3 snapshot
sudo target/release/singlevm \
    --kernel resources/images/vmlinux-4.20.0 \
    --rootfs /ssd/rootfs/python3.ext4 \
    --mem_size 128 \
    --vcpu_count 1 \
    --dump_dir /ssd/snapshot/python3 \
    --force_exit
```
`--rootfs` should be the path to the corresponding root file system, either a language snapshot
version file system or a fullapp snapshot version file system.

```bash
# create the target snapshot directory
mkdir /ssd/snapshots/python3
# generate a Python3 snapshot
sudo target/release/singlevm \
    --kernel resources/images/vmlinux-4.20.0 \
    --rootfs /ssd/rootfs/python3.ext4 \
    --appfs snapfaas-images/appfs/empty/output.ext2 \
    --mem_size 128 \
    --vcpu_count 1 \
    --dump_dir /ssd/snapshot/python3 \
    --force_exit
# generate the diff snapshot
sudo target/release/singlevm \
    --kernel resources/images/vmlinux-4.20.0 \
    --rootfs /ssd/rootfs/python3.ext4 \
    --appfs /ssd/appfs/hello-python3.ext2 \
    --mem_size 128 \
    --vcpu_count 1 \
    --load_dir /ssd/snapshot/python3 \
    --dump_dir /ssd/snapshot/diff/hello-python3 \
    --force_exit
# generate working set
sudo target/release/singlevm \
    --kernel resources/images/vmlinux-4.20.0 \
    --rootfs /ssd/rootfs/python3.ext4 \
    --appfs /ssd/appfs/hello-python3.ext2 \
    --mem_size 128 \
    --vcpu_count 1 \
    --load_dir /ssd/snapshot/python3 \
    --diff_dirs /ssd/snapshot/diff/hello-python3 \
    --dump_ws /ssd/snapshot/diff/hello-python3 \
    < resources/requests/hello-python3.json
```
`rootfs` is the path to the root file system that contains a language runtime (e.g., Python3).
`appfs` is the path to the empty placeholder file system or a file system that contain the function.

## Use snapshots
```bash
# run hello function
sudo target/release/singlevm \
    --kernel resources/images/vmlinux-4.20.0 \
    --rootfs /ssd/rootfs/python3.ext4 \
    --appfs snapfaas-images/appfs/hellopy2/output.ext2 \
    --mem_size 128 \
    --vcpu_count 1 \
    --load_dir /ssd/snapshot/python3 \
    [--diff_dirs /ssd/snapshot/diff/hello-python3] \
    [--load_ws] < resources/requests/hello-python3.json
```
Option `--diff_dirs` is optional. Its presence dictates the diff snapshot should be used.

Option `--load_ws` is optional. Its presence dictates only the working set should be eagerly loaded.
