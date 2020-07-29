# create snapfaas group and allow current user to access /etc/snapfaas
if [ $(cat /etc/group | grep snapfaas | wc -l) -eq 0 ]; then
    sudo groupadd -f snapfaas
    sudo usermod -a -G snapfaas $USER
    sudo mkdir -p /etc/snapfaas
    sudo chown $USER:snapfaas /etc/snapfaas
    sudo chmod 775 /etc/snapfaas
fi

# creating directories if not already exist
[ ! -d /etc/snapfaas/images ] && mkdir /etc/snapfaas/images
[ ! -d /etc/snapfaas/images/runtimefs ] && mkdir /etc/snapfaas/images/runtimefs
[ ! -d /etc/snapfaas/images/appfs ] && mkdir /etc/snapfaas/images/appfs
[ ! -d /etc/snapfaas/images/snapshot ] && mkdir /etc/snapfaas/images/snapshot
[ ! -d out ] && mkdir out

CONTROLLER_CONFIG=example-controller-config.yaml
echo 'Copying configuration files to /etc/snapfaas ...'
cp ./resources/$CONTROLLER_CONFIG /etc/snapfaas
cp ./resources/example-function-configs.yaml /etc/snapfaas
echo 'Copying uncompressed kernel binary to /etc/snapfaas/images ...'
cp ./resources/vmlinux-4.20.0 /etc/snapfaas/images

echo 'Building rootfs ...'
cd snapfaas-images/separate
./mk_rtimage.sh python3-net-vsock /etc/snapfaas/images/runtimefs/python3.ext4 1>/dev/null
if [ $? -ne 0 ]; then
    echo 'Failed to build rootfs'
    exit 1
fi
cd ../..

echo 'Building hello appfs...'
make -C ./snapfaas-images/appfs/hellopy2 1>/dev/null
cp ./snapfaas-images/appfs/hellopy2/output.ext2 /etc/snapfaas/images/appfs/hello.ext2

echo 'Building firerunner...'
cargo build --release --bin firerunner 1>/dev/null
cp ./target/release/firerunner /etc/snapfaas

echo 'Building snapctr...'
cargo build --release --bin snapctr 1>/dev/null

echo 'Launching example workload in resources/example-requests.json:'
echo 'Total memory = 1024 MB'
echo 'VM size = 128 MB'
echo 'pool size = 8 workers'
echo 'Setting up tap devices for 8 workers...'
scripts/setup-tap-bridge.sh 8
echo 'Start example workload...'
target/release/snapctr --config /etc/snapfaas/$CONTROLLER_CONFIG --mem 1024 --requests_file resources/example-requests.json
echo 'Cleaning up all tap devices...'
scripts/cleanup-taps.sh 8
echo 'Cleaning up all unix domain socket listeners...'
rm worker*
echo 'Done'
