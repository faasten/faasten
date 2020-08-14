#!/bin/bash

if [ ! -d microbenchmark ]; then
    echo 'Working directory should be snapfaas'
    exit 1
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
./mk_rtimage.sh python3 ../../tmp/images/python3.ext4 &>/dev/null
./mk_rtimage.sh nodejs ../../tmp/images/nodejs.ext4 &>/dev/null
cd ../..

echo 'Building base snapshots...'
runtimes=( python3 nodejs )
make -C snapfaas-images/appfs/empty &>/dev/null
for runtime in "${runtimes[@]}"
do
    [ ! -d tmp/images/snapshot/$runtime ] && mkdir tmp/images/snapshot/$runtime
    sudo tmp/bin/release/fc_wrapper \
        --vcpu_count 1 \
        --mem_size 128 \
        --kernel tmp/images/vmlinux-4.20.0 \
        --rootfs tmp/images/$runtime.ext4 \
        --appfs snapfaas-images/appfs/empty/output.ext2 \
        --dump_dir tmp/images/snapshot/$runtime \
        --firerunner tmp/bin/release/firerunner \
        --network 'tap0/aa:bb:cc:dd:ff:00' &>/dev/null
    [ $? -ne 0 ] && echo 'failed to build base snapshot' && exit 1
done

echo 'Building appfs...'
[ ! -d /ssd/images/appfs ] && mkdir -p /ssd/images/appfs
appfsDir='snapfaas-images/appfs'
for runtime in "${runtimes[@]}"
do
    for app in $(dir $appfsDir/$runtime)
    do
        if [ $app != 'noop-carray' ]; then
            make -C $appfsDir/$runtime/$app &>/dev/null
            cp $appfsDir/$runtime/$app/output.ext2 /ssd/images/appfs/$app-$runtime.ext2
        fi
    done
done

echo 'Building diff snapshots...'
[ ! -d /ssd/images/snapshot/diff ] && mkdir -p /ssd/images/snapshot/diff
for runtime in "${runtimes[@]}"
do
    for app in $(dir $appfsDir/$runtime)
    do
        if [[ $app =~ ^\. ]]; then
            continue
        fi
        [ ! -d /ssd/images/snapshot/diff/$app-$runtime ] && mkdir /ssd/images/snapshot/diff/$app-$runtime
        sudo tmp/bin/release/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel tmp/images/vmlinux-4.20.0 \
            --rootfs tmp/images/python3.ext4 \
            --appfs $appfsDir/$runtime/$app/output.ext2 \
            --load_dir tmp/images/snapshot/python3 \
            --dump_dir /ssd/images/snapshot/diff/$app-$runtime \
            --firerunner tmp/bin/release/firerunner \
            --network 'tap0/aa:bb:cc:dd:ff:00' &>/dev/null
        [ $? -eq 1 ] && echo 'error: failed to build diff snapshot for' $app && exit 1
    done
done
