#!/usr/bin/env bash

source ./default_env

echo 'Building debug base snapshots...'
make -C ../snapfaas-images/appfs/empty &>/dev/null
for runtime in "${RUNTIMES[@]}"
do
    echo "- $SSDSNAPSHOTDIR/$runtime-debug"
    [ ! -d $SSDSNAPSHOTDIR/$runtime-debug ] && mkdir $SSDSNAPSHOTDIR/$runtime-debug
    sudo $MEMBINDIR/fc_wrapper \
        --vcpu_count 1 \
        --mem_size 128 \
        --kernel_args 'console=ttyS0 quiet' \
        --kernel $KERNEL \
        --network $NETDEV \
        --firerunner $MEMBINDIR/firerunner \
        --rootfs $SSDROOTFSDIR/snapfaas/$runtime.ext4 \
        --appfs ../snapfaas-images/appfs/empty/output.ext2 \
        --dump_dir $SSDSNAPSHOTDIR/$runtime-debug \
        --force_exit &>/dev/null
    [ $? -ne 0 ] && echo '!! failed' && exit 1
done

echo 'Building debug diff snapshots...'
appfsDir=../snapfaas-images/appfs
for runtime in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$runtime)
    do
        echo "- $SSDSNAPSHOTDIR/diff/$app-$runtime-debug"
        [ ! -d $SSDSNAPSHOTDIR/diff/$app-$runtime-debug ] && mkdir -p $SSDSNAPSHOTDIR/diff/$app-$runtime-debug
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --network $NETDEV \
            --rootfs $SSDROOTFSDIR/snapfaas/$runtime.ext4 \
            --appfs $SSDAPPFSDIR/$app-$runtime.ext2 \
            --load_dir $SSDSNAPSHOTDIR/$runtime-debug \
            --dump_dir $SSDSNAPSHOTDIR/diff/$app-$runtime-debug \
            --force_exit &>/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1
    done
done
exit 0
