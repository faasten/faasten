#!/usr/bin/env bash

# check the existence of /ssd directory
# and make sure /ssd is on an SSD device
if [ ! -d /ssd ]; then
    echo '/ssd must exists'
    exit 1
fi
mountpoint -q /ssd
if [ $? -eq 1 ]; then
    echo 'INFO: /ssd is not a mountpoint, checking if the root block device is an SSD...'
    if [ $(lsblk -o mountpoint,rota | egrep "^/ +" | awk '{ print $2 }') -ne 0 ]; then
        echo 'ERROT: the root device is not an SSD'
        exit 1
    fi
else 
    if [ $(lsblk -o rota,mountpoint | egrep /ssd | awk '{ print $1 }') -ne 0 ]; then
	echo 'ERROR: the device mounted to /ssd is not an SSD device.'
	exit 1
    fi
fi

source ./env
echo 'creating directories...'
# tmpfs
echo "mounting 20GB tmpfs at $MOUNTPOINT"
[ ! -d $MOUNTPOINT ] && mkdir $MOUNTPOINT
mountpoint -q $MOUNTPOINT
[ $? -eq 1 ] && sudo mount -t tmpfs -o size=20G tmpfs $MOUNTPOINT
[ ! -d  $MEMSNAPSHOTDIR ] && mkdir -p $MEMSNAPSHOTDIR && mkdir $MEMSNAPSHOTDIR/diff
[ ! -d $MEMROOTFSDIR ] && mkdir -p $MEMROOTFSDIR
[ ! -d $MEMAPPFSDIR ] && mkdir -p $MEMAPPFSDIR
[ ! -d $MEMBINDIR ] && mkdir -p $MEMBINDIR
# /ssd
[ ! -d $SSDSNAPSHOTDIR ] && mkdir -p $SSDSNAPSHOTDIR
[ ! -d $SSDROOTFSDIR ] && mkdir -p $SSDROOTFSDIR
[ ! -d $SSDAPPFSDIR ] && mkdir -p $SSDAPPFSDIR
[ ! -d $SSDEXECSNAPSHOTDIR ] && mkdir -p $SSDEXECSNAPSHOTDIR


cp ../resources/images/vmlinux-4.20.0 $MOUNTPOINT/images

echo 'Deploying alexa-door device...'
full_path=$(dirname $(realpath $0))
cd ../snapfaas-images/appfs/nodejs/alexa-door/door-device
echo "Switching to directory $PWD..."
./deploy.sh
cd $full_path
echo "Switching to directory $PWD..."

echo 'Configuring tap0...'
../scripts/setup-tap-bridge.sh 1

# build firerunner/fc_wrapper binaries
setup_scripts/build_binaries.sh
# build root filesystems
setup_scripts/build_rootfs.sh
# build app filesystems
setup_scripts/build_appfs.sh
# build language base snapshots + app diff snapshots
setup_scripts/build_diff_snapshots.sh
# build full-app snapshots
setup_scripts/build_fullapp_snapshots.sh
# build diff snapshots with 'console=ttyS0' boot command line argument
setup_scripts/build_debug_diff_snapshots.sh
echo 'setup' > .stat
