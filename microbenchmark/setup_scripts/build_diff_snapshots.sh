#!/usr/bin/env bash

source ./default_env

echo 'creating snapshot directories...'
[ ! -d $MEMSNAPSHOTDIR ] && mkdir -p $MEMSNAPSHOTDIR/diff
[ ! -d $SSDSNAPSHOTDIR ] && mkdir -p $SSDSNAPSHOTDIR/diff
[ ! -d $HDDSNAPSHOTDIR ] && mkdir -p $HDDSNAPSHOTDIR/diff

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
        --rootfs $SSDROOTFSDIR/snapfaas/$runtime.ext4 \
        --appfs ../snapfaas-images/appfs/empty/output.ext2 \
        --dump_dir $MEMSNAPSHOTDIR/$runtime \
        --force_exit >/dev/null
    [ $? -ne 0 ] && echo '!! failed' && exit 1

    echo "- $SSDSNAPSHOTDIR/$runtime"
    [ ! -d $SSDSNAPSHOTDIR/$runtime ] && mkdir $SSDSNAPSHOTDIR/$runtime
    cp $MEMSNAPSHOTDIR/$runtime/* $SSDSNAPSHOTDIR/$runtime
    echo "- $HDDSNAPSHOTDIR/$runtime"
    [ ! -d $HDDSNAPSHOTDIR/$runtime ] && mkdir $HDDSNAPSHOTDIR/$runtime
    cp $MEMSNAPSHOTDIR/$runtime/* $HDDSNAPSHOTDIR/$runtime
done

echo 'Building diff snapshots...'
appfsDir=../snapfaas-images/appfs
for runtime in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$runtime)
    do
        echo "- $SSDSNAPSHOTDIR/diff/$app-$runtime"
        [ ! -d $SSDSNAPSHOTDIR/diff/$app-$runtime ] && mkdir -p $SSDSNAPSHOTDIR/diff/$app-$runtime
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --network $NETDEV \
            --rootfs $SSDROOTFSDIR/snapfaas/$runtime.ext4 \
            --appfs $SSDAPPFSDIR/$app-$runtime.ext2 \
            --load_dir $SSDSNAPSHOTDIR/$runtime \
            --dump_dir $SSDSNAPSHOTDIR/diff/$app-$runtime \
            --force_exit >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1

        echo "- $MEMSNAPSHOTDIR/diff/$app-$runtime"
        [ ! -d $MEMSNAPSHOTDIR/diff/$app-$runtime ] && mkdir $MEMSNAPSHOTDIR/diff/$app-$runtime
        cp $SSDSNAPSHOTDIR/diff/$app-$runtime/* $MEMSNAPSHOTDIR/diff/$app-$runtime
        echo "- $HDDSNAPSHOTDIR/diff/$app-$runtime"
        [ ! -d $HDDSNAPSHOTDIR/diff/$app-$runtime ] && mkdir $HDDSNAPSHOTDIR/diff/$app-$runtime
        cp $SSDSNAPSHOTDIR/diff/$app-$runtime/* $HDDSNAPSHOTDIR/diff/$app-$runtime
    done
done
