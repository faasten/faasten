#!/bin/bash

if [ ! -d /ssd ]; then
    echo '/ssd must exist'
    exit 1
fi

runtimes=( python3 nodejs )
source ./env
echo 'Generating language snapshots...'
for runtime in "${runtimes[@]}"
do
    echo "- $SSDEXECSNAPSHOTDIR/$runtime"
    [ ! -d $SSDEXECSNAPSHOTDIR/$runtime ] && mkdir $SSDEXECSNAPSHOTDIR/$runtime
    sudo $MEMBINDIR/fc_wrapper \
        --kernel $KERNEL \
        --rootfs $SSDROOTFSDIR/$runtime-exec.ext4 \
        --appfs ../snapfaas-images/appfs/empty/output.ext2 \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --mem_size 128 \
        --vcpu_count 1 \
        --dump_dir $SSDEXECSNAPSHOTDIR/$runtime \
        --firerunner $MEMBINDIR/firerunner \
        --force_exit &>/dev/null
    [ $? -ne 0 ] && echo '!! failed' && exit 1
done

echo 'Generating diff snapshots...'
for runtime in "${runtimes[@]}"
do
    apps=$(ls ../snapfaas-images/appfs/$runtime)
    for app in $apps
    do
        [ ! -d $SSDEXECSNAPSHOTDIR/diff/$app-$runtime-func ] && mkdir -p $SSDEXECSNAPSHOTDIR/diff/$app-$runtime-func
        [ ! -d $SSDEXECSNAPSHOTDIR/diff/$app-$runtime-exec ] && mkdir -p $SSDEXECSNAPSHOTDIR/diff/$app-$runtime-exec
        echo "- $app-$runtime-func"
        sudo $MEMBINDIR/fc_wrapper \
            --kernel $KERNEL \
            --rootfs $SSDROOTFSDIR/$runtime-exec.ext4 \
            --appfs $SSDAPPFSDIR/$app-$runtime.ext2 \
            --network 'tap0/aa:bb:cc:dd:ff:ff' \
            --mem_size 128 \
            --vcpu_count 1 \
            --load_dir $SSDEXECSNAPSHOTDIR/$runtime \
            --firerunner $MEMBINDIR/firerunner \
            --dump_dir $SSDEXECSNAPSHOTDIR/diff/$app-$runtime-func \
            --force_exit >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1

        echo "- $app-$runtime-exec"
        sudo $MEMBINDIR/fc_wrapper \
            --kernel $KERNEL \
            --rootfs $SSDROOTFSDIR/$runtime-exec.ext4 \
            --appfs $SSDAPPFSDIR/$app-$runtime.ext2 \
            --network 'tap0/aa:bb:cc:dd:ff:ff' \
            --mem_size 128 \
            --vcpu_count 1 \
            --load_dir $SSDEXECSNAPSHOTDIR/$runtime \
            --firerunner $MEMBINDIR/firerunner \
            --diff_dirs $SSDEXECSNAPSHOTDIR/diff/$app-$runtime-func \
            --dump_dir $SSDEXECSNAPSHOTDIR/diff/$app-$runtime-exec < ../resources/requests/$app-$runtime.json &>/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1
    done
done

echo 'Writing results to ratio.txt'
>ratio.txt
for runtime in "${runtimes[@]}"
do
    for app in $(ls ../snapfaas-images/appfs/$runtime)
    do
        echo $app-$runtime >> ratio.txt
        ../snapfaas-images/python_scripts/parse_page_numbers.py $SSDEXECSNAPSHOTDIR/$runtime/page_numbers,$SSDEXECSNAPSHOTDIR/diff/$app-$runtime-func/page_numbers,$SSDEXECSNAPSHOTDIR/diff/$app-$runtime-exec/page_numbers >> ratio.txt
    done
done
