## Machine Setup
SnapFaaS depends on `docker` so make sure `docker` is installed.
### launch docker service
With `docker` installed, start `dockerd`:
```bash
sudo groupadd docker
sudo service docker start
sudo usermod -aG docker $USER
```
The last line gives access permissions to `$USER`
but requires the user to log out the current ssh session and then log back in
to take effect.
### network setup
SnapFaaS uses `docker0` bridge. `docker0` should be automatically
up when `docker` service is launched.
```bash
scripts/setup-tap-bridge.sh
```

## Build Binaries
This repo contains binaries `snapctr`, `fc_wrapper`, and `firerunner`.

To build binary `snapctr`, execute
```bash
cargo build --release --bin snapctr
```
To build binary `fc_wrapper`, execute
```bash
cargo build --release --bin fc_wrapper
```
To build binary `firerunner`, execute
```bash
cargo build --release --bin firerunner
```

Following the instructions above places all binaries under `target/release` directory.

## Build root filesystems and application filesystems
SnapFaaS uses `docker` to build both kinds of filesystems.
The Linux distro used is Alpine Linux v3.10.
### root filesystem
Currently only Python3.7 is supported. To build a root filesystem for Python3.7, execute
```bash
cd snapfaas-images/separate
# replace the path `/ssd/rootfs/python3.ext4` with the path at which you want to place the root filesystem.
./mk_rt_images python3-net-vsock /ssd/rootfs/python3.ext4
```
## application filesystem
```bash
# use hello as example
cd snapfaas-images/appfs/hellopy2
make
```
The command above generates `output.ext2` in the `hellopy2` directory.

## Execution
`firerunner` should *not* be directly executed through command line.

`snapctr` and `fc_wrapper` both internally execute `firerunner` binary. `firerunner` binary is
expected to be at `target/release` directory, which is the case if the build instructions above are followed.

### Launch a single VM
`fc_wrapper` launches a single VM instance and reads in requests from `stdin`.
It is good for testing out new language and new applications.
Users should use `fc_wrapper` to generate VM snapshots.

1. To conduct a regular boot, execute:
```bash
# run hello function
sudo target/release/fc_wrapper \
    --kernel resources/vmlinux-4.20.0 \
    --rootfs /ssd/ext4/python3.ext4 \
    --appfs snapfaas-images/appfs/hellopy2/output.ext2 \
    --network 'tap0/ff:ff:ff:ff:ff:ff' \
    --mem_size 128 \
    --vcpu_count 1 < resources/json-hello
```
2. To generate a Python3 snapshot, execute:
```bash
# create the target snapshot directory
mkdir /ssd/snapshots/python3
# generate a snapshot
sudo target/release/fc_wrapper \
    --kernel resources/vmlinux-4.20.0 \
    --rootfs /ssd/ext4/python3.ext4 \
    --appfs snapfaas-images/appfs/empty/output.ext2 \
    --network 'tap0/ff:ff:ff:ff:ff:ff' \
    --mem_size 128 \
    --vcpu_count 1 \
    --dump_dir /ssd/snapshots/python3
```
When users see the line "Snapshot generation succeeds",
they should Ctrl-C to terminate the process as the process
won't exit on itself currently.

1. To generate a diff snapshot for a function, execute:
```bash
# create the target snapshot directory
mkdir /ssd/snapshots/diff/hello
# generate the diff snapshot
sudo target/release/fc_wrapper \
    --kernel resources/vmlinux-4.20.0 \
    --rootfs /ssd/ext4/python3.ext4 \
    --appfs snapfaas-images/appfs/hellopy2/output.ext2 \
    --network 'tap0/ff:ff:ff:ff:ff:ff' \
    --mem_size 128 \
    --vcpu_count 1 \
    --load_dir /ssd/snapshots/python3 \
    --dump_dir /ssd/snapshots/diff/hello
```
4. To boot a VM from a snapshot, execute:
```bash
# run hello function
sudo target/release/fc_wrapper \
    --kernel resources/vmlinux-4.20.0 \
    --rootfs /ssd/ext4/python3.ext4 \
    --appfs snapfaas-images/appfs/hellopy2/output.ext2 \
    --network 'tap0/ff:ff:ff:ff:ff:ff' \
    --mem_size 128 \
    --vcpu_count 1 \
    --load_dir /ssd/snapshots/python3 \
    --diff_dirs /ssd/snapshots/diff/hello < resources/json-hello
```

5. For debugging, one can turn on guest VM console allowing guest VM output,
```bash
sudo target/release/fc_wrapper \
    --kernel resources/vmlinux-4.20.0 \
    --kernel_args 'console=ttyS0' \
    --rootfs /ssd/ext4/python3.ext4 \
    --appfs snapfaas-images/appfs/hellopy2/output.ext2 \
    --network 'tap0/ff:ff:ff:ff:ff:ff' \
    --mem_size 128 \
    --vcpu_count 1 < resources/json-hello
```
### snapctr
`snapctr` should be used to run 
1. example controller configuration file `resources/example-controller.config`
```yaml
```
2. example function configuration file `resources/example-function.config`
```yaml
```
3. execution
```bash
sudo target/release/snapctr --config resources/
```
