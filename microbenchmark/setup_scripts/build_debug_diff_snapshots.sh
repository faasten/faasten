#!/usr/bin/env bash

source ./default_env

echo 'Building debug base snapshots...'
make -C ../snapfaas-images/appfs/empty &>/dev/null
for runtime in "${RUNTIMES[@]}"
do
    echo "- $MEMSNAPSHOTDIR/$runtime-debug"
    [ ! -d $MEMSNAPSHOTDIR/$runtime-debug ] && mkdir $MEMSNAPSHOTDIR/$runtime-debug
    sudo $MEMBINDIR/fc_wrapper \
        --vcpu_count 1 \
        --mem_size 128 \
        --kernel_args 'console=ttyS0 quiet' \
        --kernel $KERNEL \
        --network $NETDEV \
        --firerunner $MEMBINDIR/firerunner \
        --rootfs $MEMROOTFSDIR/snapfaas/$runtime.ext4 \
        --appfs ../snapfaas-images/appfs/empty/output.ext2 \
        --dump_dir $MEMSNAPSHOTDIR/$runtime-debug \
        --no_odirect_root \
        --force_exit &>/dev/null
    [ $? -ne 0 ] && echo '!! failed' && exit 1
done

echo 'Building debug diff snapshots...'
appfsDir=../snapfaas-images/appfs
for runtime in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$runtime)
    do
        echo "- $MEMSNAPSHOTDIR/diff/$app-$runtime-debug"
        [ ! -d $MEMSNAPSHOTDIR/diff/$app-$runtime-debug ] && mkdir -p $MEMSNAPSHOTDIR/diff/$app-$runtime-debug
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --network $NETDEV \
            --rootfs $MEMROOTFSDIR/snapfaas/$runtime.ext4 \
            --appfs $MEMAPPFSDIR/$app-$runtime.ext2 \
            --load_dir $MEMSNAPSHOTDIR/$runtime-debug \
            --dump_dir $MEMSNAPSHOTDIR/diff/$app-$runtime-debug \
            --no_odirect_root --no_odirect_app --no_odirect_diff \\
            --force_exit &>/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1
    done
done
exit 0
