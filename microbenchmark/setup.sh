#!/bin/bash

if [ $(basename $PWD) != 'snapfaas' ]; then
    if [ $(basename $PWD) == 'microbenchmark' ]; then
        cd ..
    else
        echo 'Working directory should be snapfaas or snapfaas/microbenchmark'
        exit 1
    fi
fi
if [ ! -d /ssd ]; then
    echo '/ssd must exists'
    exit 1
fi


[ ! -d tmp ] && mkdir tmp
mountpoint -q tmp
[ $? -eq 1 ] && sudo mount -t tmpfs -o size=20G tmpfs tmp

[ ! -d tmp/images/snapshot ] && mkdir -p tmp/images/snapshot
[ ! -d tmp/bin ] && mkdir tmp/bin

cp resources/images/vmlinux-4.20.0 tmp/images

echo 'Building fc_wrapper and firerunner...'
cargo build --release --quiet --target-dir tmp/bin --bin fc_wrapper --bin firerunner

cd snapfaas-images/separate
./mk_rtimage.sh python3-net-vsock ../../tmp/images/python3.ext4 &>/dev/null
cd ../..

echo 'Building base snapshots...'
make -C snapfaas-images/appfs/empty &>/dev/null
[ ! -d tmp/images/snapshot/python3 ] && mkdir tmp/images/snapshot/python3
sudo tmp/bin/release/fc_wrapper \
    --vcpu_count 1 \
    --mem_size 128 \
    --kernel tmp/images/vmlinux-4.20.0 \
    --rootfs tmp/images/python3.ext4 \
    --appfs snapfaas-images/appfs/empty/output.ext2 \
    --dump_dir tmp/images/snapshot/python3 \
    --firerunner tmp/bin/release/firerunner \
    --network 'tap0/aa:bb:cc:dd:ff:00' &>/dev/null
[ $? -ne 0 ] && echo 'failed to build base snapshot' && exit 1

echo 'Building appfs...'
[ ! -d /ssd/images/appfs ] && mkdir -p /ssd/images/appfs
languages=( python3 )
appfsDir='snapfaas-images/appfs'
for language in "${languages[@]}"
do
    for app in $(dir $appfsDir/$language)
    do
        if [ $app != 'noop-carray' ]; then
            make -C $appfsDir/$language/$app &>/dev/null
            cp $appfsDir/$language/$app/output.ext2 /ssd/images/appfs/$app-$language.ext2
        fi
    done
done

echo 'Building diff snapshots...'
[ ! -d /ssd/images/snapshot/diff ] && mkdir -p /ssd/images/snapshot/diff
languages=( python3 )
for language in "${languages[@]}"
do
    for app in $(dir $appfsDir/$language)
    do
        if [ $app != 'noop-carray' ]; then
            [ ! -d /ssd/images/snapshot/diff/$app-$language ] && mkdir /ssd/images/snapshot/diff/$app-$language
            sudo tmp/bin/release/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel tmp/images/vmlinux-4.20.0 \
                --rootfs tmp/images/python3.ext4 \
                --appfs $appfsDir/$language/$app/output.ext2 \
                --load_dir tmp/images/snapshot/python3 \
                --dump_dir /ssd/images/snapshot/diff/$app-$language \
                --firerunner tmp/bin/release/firerunner \
                --network 'tap0/aa:bb:cc:dd:ff:00' &>/dev/null
            [ $? -eq 1 ] && echo 'error: failed to build diff snapshot for' $app && exit 1
        fi
    done
done
