[ ! -d out ] && mkdir out

CONTROLLER_CONFIG=example-controller-config-snapshot.yaml

echo 'Building rootfs ...'
cd snapfaas-images/separate
./mk_rtimage.sh python3-net-vsock ../../resources/images/runtimefs/python3.ext4 1>/dev/null
if [ $? -ne 0 ]; then
    echo 'Failed to build rootfs'
    exit 1
fi
cd ../..

echo 'Building placeholder appfs...'
make -C ./snapfaas-images/appfs/empty &>/dev/null
echo 'Building hello appfs...'
make -C ./snapfaas-images/appfs/hellopy2 &>/dev/null
cp ./snapfaas-images/appfs/hellopy2/output.ext2 ./resources/images/appfs/hello.ext2

echo 'Building fc_wrapper...'
cargo build --release --bin fc_wrapper &>/dev/null
echo 'Generating Python3 base snapshot...'
[ ! -d ./resources/images/snapshot/python3 ] && mkdir ./resources/images/snapshot/python3
sudo target/release/fc_wrapper \
        --kernel resources/images/vmlinux-4.20.0 \
        --rootfs resources/images/runtimefs/python3.ext4 \
        --appfs snapfaas-images/appfs/empty/output.ext2 \
        --network 'tap0/ff:ff:ff:ff:ff:ff' \
        --mem_size 128 \
        --vcpu_count 1 \
        --dump_dir resources/images/snapshot/python3
# clean up unix domain socket listeners
sudo rm worker*

echo 'Generating hello diff snapshot...'
[ ! -d resources/images/snapshot/diff/hello ] && mkdir -p resources/images/snapshot/diff/hello
sudo target/release/fc_wrapper \
        --kernel resources/images/vmlinux-4.20.0 \
        --rootfs resources/images/runtimefs/python3.ext4 \
        --appfs snapfaas-images/appfs/hellopy2/output.ext2 \
        --network 'tap0/ff:ff:ff:ff:ff:ff' \
        --mem_size 128 \
        --vcpu_count 1 \
        --load_dir resources/images/snapshot/python3 \
        --dump_dir resources/images/snapshot/diff/hello
# clean up unix domain socket listeners
sudo rm worker*

echo 'Building firerunner...'
cargo build --release --bin firerunner &>/dev/null

echo 'Building snapctr...'
cargo build --release --bin snapctr &>/dev/null

echo 'Launching example workload in resources/example-requests.json:'
echo 'Total memory = 1024 MB'
echo 'VM size = 128 MB'
echo 'pool size = 8 workers'
echo 'Setting up tap devices for 8 workers...'
scripts/setup-tap-bridge.sh 8
echo 'Start example workload...'
sudo target/release/snapctr --config resources/$CONTROLLER_CONFIG --mem 1024 --requests_file resources/example-requests.json
echo 'Cleaning up all tap devices...'
scripts/cleanup-taps.sh 8
echo 'Cleaning up all unix domain socket listeners...'
sudo rm worker*
echo 'Done'
