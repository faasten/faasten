#!/usr/bin/env bash

if [ ! -d /ssd ]; then
    echo '/ssd must exists'
    exit 1
fi

source ./env
echo 'Building debug base snapshots...'
runtimes=( python3 nodejs )
make -C ../snapfaas-images/appfs/empty &>/dev/null
for runtime in "${runtimes[@]}"
do
    echo "- $MEMSNAPSHOTDIR/$runtime-debug"
    [ ! -d $MEMSNAPSHOTDIR/$runtime-debug ] && mkdir $MEMSNAPSHOTDIR/$runtime-debug
    sudo $MEMBINDIR/fc_wrapper \
        --vcpu_count 1 \
        --mem_size 128 \
        --kernel_args 'console=ttyS0' \
        --kernel $KERNEL \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --firerunner $MEMBINDIR/firerunner \
        --rootfs $SSDROOTFSDIR/$runtime.ext4 \
        --appfs ../snapfaas-images/appfs/empty/output.ext2 \
        --dump_dir $MEMSNAPSHOTDIR/$runtime-debug \
        --force_exit &>/dev/null
    [ $? -ne 0 ] && echo '!! failed' && exit 1
done

echo 'Building debug diff snapshots...'
appfsDir=../snapfaas-images/appfs
for runtime in python3 nodejs
do
    for app in $(ls $appfsDir/$runtime)
    do
        echo "$SSDSNAPSHOTDIR/diff/$app-$runtime-debug"
        [ ! -d $SSDSNAPSHOTDIR/diff/$app-$runtime-debug ] && mkdir -p $SSDSNAPSHOTDIR/diff/$app-$runtime-debug
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
