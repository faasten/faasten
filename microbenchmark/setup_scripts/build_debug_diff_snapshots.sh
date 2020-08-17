#!/usr/bin/env bash

if [ ! -d /ssd ]; then
    echo '/ssd must exists'
    exit 1
fi

source ./env
echo 'Building debug diff snapshots...'
appfsDir=../snapfaas-images/appfs
for runtime in python3 nodejs
do
    for app in $(ls $appfsDir/$runtime)
    do
        echo "$SSDSNAPSHOTDIR/diff/$app-$runtime-debug"
        [ ! -d $SSDSNAPSHOTDIR/diff/$app-$runtime ] && mkdir -p $SSDSNAPSHOTDIR/diff/$app-$runtime
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --network 'tap0/aa:bb:cc:dd:ff:00' \
            --rootfs $SSDROOTFSDIR/$runtime.ext4 \
            --appfs $SSDAPPFSDIR/$app-$runtime.ext2 \
            --load_dir $MEMSNAPSHOTDIR/$runtime-debug \
            --dump_dir $SSDSNAPSHOTDIR/diff/$app-$runtime-debug \
            --force_exit &>/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1
    done
done
