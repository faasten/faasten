#!/usr/bin/env bash

source ./default_env

echo 'creating snapshot directories...'
[ ! -d $MEMSNAPSHOTDIR ] && mkdir -p $MEMSNAPSHOTDIR/diff

echo 'Building base snapshots...'
make -C ../snapfaas-images/appfs/empty &>/dev/null
for runtime in "${RUNTIMES[@]}"
do
    echo "- $MEMSNAPSHOTDIR/$runtime"
    [ ! -d $MEMSNAPSHOTDIR/$runtime ] && mkdir $MEMSNAPSHOTDIR/$runtime
    sudo $MEMBINDIR/fc_wrapper \
        --vcpu_count 1 \
        --mem_size 128 \
        --kernel $KERNEL \
        --network $NETDEV \
        --firerunner $MEMBINDIR/firerunner \
        --rootfs $MEMROOTFSDIR/snapfaas/$runtime.ext4 \
        --appfs ../snapfaas-images/appfs/empty/output.ext2 \
        --dump_dir $MEMSNAPSHOTDIR/$runtime \
        --no_odirect_root \
        --force_exit >/dev/null
    [ $? -ne 0 ] && echo '!! failed' && exit 1

done

echo 'Building diff snapshots...'
appfsDir=../snapfaas-images/appfs
for runtime in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$runtime)
    do
        echo "- $MEMSNAPSHOTDIR/diff/$app-$runtime"
        [ ! -d $MEMSNAPSHOTDIR/diff/$app-$runtime ] && mkdir $MEMSNAPSHOTDIR/diff/$app-$runtime
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --network $NETDEV \
            --rootfs $MEMROOTFSDIR/snapfaas/$runtime.ext4 \
            --appfs $MEMAPPFSDIR/$app-$runtime.ext2 \
            --load_dir $MEMSNAPSHOTDIR/$runtime \
            --dump_dir $MEMSNAPSHOTDIR/diff/$app-$runtime \
            --no_odirect_root --no_odirect_app --no_odirect_diff \
            --force_exit >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1
    done
done

echo 'Building WS...'
appfsDir=../snapfaas-images/appfs
for runtime in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$runtime)
    do
        echo "- $MEMSNAPSHOTDIR/diff/$app-$runtime"
        [ ! -d $MEMSNAPSHOTDIR/diff/$app-$runtime ] && mkdir $MEMSNAPSHOTDIR/diff/$app-$runtime
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --network $NETDEV \
            --rootfs $MEMROOTFSDIR/snapfaas/$runtime.ext4 \
            --appfs $MEMAPPFSDIR/$app-$runtime.ext2 \
            --load_dir $MEMSNAPSHOTDIR/$runtime,$MEMSNAPSHOTDIR/diff/$app-$runtime \
            --dump_ws \
            --no_odirect_root --no_odirect_app --no_odirect_diff \
            < ../resources/requests/$app-$runtime.json >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1
    done
done
exit 0
