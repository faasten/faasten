#!/usr/bin/env bash

if [ ! -d /ssd ]; then
    echo '/ssd must exists'
    exit 1
fi

# only source the environment file when invoked directly from command line
if [ $(ps -o stat= -p $PPID) == 'Ss' ]; then
	source ./default_env
fi
echo 'Building base snapshots...'
runtimes=( python3 nodejs )
make -C ../snapfaas-images/appfs/empty &>/dev/null
for runtime in "${runtimes[@]}"
do
    echo "- $MEMSNAPSHOTDIR/$runtime"
    [ ! -d $MEMSNAPSHOTDIR/$runtime ] && mkdir $MEMSNAPSHOTDIR/$runtime
    sudo $MEMBINDIR/fc_wrapper \
        --vcpu_count 1 \
        --mem_size 128 \
        --kernel $KERNEL \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --firerunner $MEMBINDIR/firerunner \
        --rootfs $SSDROOTFSDIR/$runtime.ext4 \
        --appfs ../snapfaas-images/appfs/empty/output.ext2 \
        --dump_dir $MEMSNAPSHOTDIR/$runtime \
        --force_exit >/dev/null
    [ $? -ne 0 ] && echo '!! failed' && exit 1

    echo "- $SSDSNAPSHOTDIR/$runtime"
    [ ! -d $SSDSNAPSHOTDIR/$runtime ] && mkdir $SSDSNAPSHOTDIR/$runtime
    cp $MEMSNAPSHOTDIR/$runtime/* $SSDSNAPSHOTDIR/$runtime
done

echo 'Building diff snapshots...'
appfsDir=../snapfaas-images/appfs
for runtime in "${runtimes[@]}"
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
            --network 'tap0/aa:bb:cc:dd:ff:00' \
            --rootfs $SSDROOTFSDIR/$runtime.ext4 \
            --appfs $SSDAPPFSDIR/$app-$runtime.ext2 \
            --load_dir $MEMSNAPSHOTDIR/$runtime \
            --dump_dir $SSDSNAPSHOTDIR/diff/$app-$runtime \
            --force_exit >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1

        echo "- $MEMSNAPSHOTDIR/diff/$app-$runtime"
        [ ! -d $MEMSNAPSHOTDIR/diff/$app-$runtime ] && mkdir $MEMSNAPSHOTDIR/diff/$app-$runtime
        cp $SSDSNAPSHOTDIR/diff/$app-$runtime/* $MEMSNAPSHOTDIR/diff/$app-$runtime
    done
done
