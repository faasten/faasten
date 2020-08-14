[ ! -d out ] && mkdir out

CONTROLLER_CONFIG=example-controller-config.yaml

echo 'Building rootfs ...'
cd snapfaas-images/separate
./mk_rtimage.sh python3-net-vsock ../../resources/images/runtimefs/python3.ext4 1>/dev/null
if [ $? -ne 0 ]; then
    echo 'Failed to build rootfs'
    exit 1
fi
cd ../..

echo 'Building hello appfs...'
make -C ./snapfaas-images/appfs/hellopy2 1>/dev/null
cp ./snapfaas-images/appfs/hellopy2/output.ext2 ./resources/images/appfs/hello.ext2

echo 'Building firerunner...'
cargo build --release --bin firerunner 1>/dev/null

echo 'Building snapctr...'
cargo build --release --bin snapctr 1>/dev/null

echo 'Launching example workload in resources/example-requests.json:'
echo "    Total memory = 1024 MB"
echo "    VM size = 128 MB"
echo "    pool size = 8 workers"
echo "    Setting up 8 tap devices for 8 workers..."
scripts/setup-tap-bridge.sh 8
echo 'Start example workload...'
sudo target/release/snapctr --config resources/$CONTROLLER_CONFIG --mem 1024 --requests_file resources/example-requests.json
echo 'Cleaning up all tap devices...'
scripts/cleanup-taps.sh 8
echo 'Cleaning up all unix domain socket listeners...'
sudo rm worker*
echo 'Done'
