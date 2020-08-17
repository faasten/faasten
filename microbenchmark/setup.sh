#!/usr/bin/env bash

if [ ! -d /ssd ]; then
    echo '/ssd must exists'
    exit 1
fi
mountpoint -q /ssd
if [ $? -eq 1 ]; then
    echo 'WARNING: an SSD device should be mounted to /ssd'
    exit 1
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
[ ! -d $MEMBINDIR ] && mkdir $MEMBINDIR
# /ssd
[ ! -d $SSDSNAPSHOTDIR ] && mkdir -p $SSDSNAPSHOTDIR
[ ! -d $SSDROOTFSDIR ] && mkdir -p $SSDROOTFSDIR
[ ! -d $SSDAPPFSDIR ] && mkdir -p $SSDAPPFSDIR
[ ! -d $SSDEXECSNAPSHOTDIR ] && mkdir -p $SSDEXECSNAPSHOTDIR


cp ../resources/images/vmlinux-4.20.0 $MOUNTPOINT/images

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
# build base snapshots with 'console=ttyS0' boot command line argument
setup_scripts/build_debug_base_snapshots.sh
# build diff snapshots with 'console=ttyS0' boot command line argument
setup_scripts/build_debug_diff_snapshots.sh
echo 'setup' > .stat
