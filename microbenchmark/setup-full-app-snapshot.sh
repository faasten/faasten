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

cargo build --release --target-dir tmp/bin --quiet --bin fc_wrapper --bin firerunner

echo 'Building combined appfs...'
cd snapfaas-images/combined
[ ! -d /ssd/images/rootfs ] && mkdir -p /ssd/images/rootfs
languages=( python3 )
appfsDir='../appfs'
for language in "${languages[@]}"
do
    apps=$(ls $appfsDir/$language -I noop-carray)
    for app in $apps
    do
        ./mk_appimage.sh python3-net-vsock /ssd/images/rootfs/$app-$language.ext4 $appfsDir/$language/$app &>/dev/null
    done
done

cd ../..
appfsDir='snapfaas-images/appfs'
echo 'Building full app snapshots...'
for language in "${languages[@]}"
do
    apps=$(ls $appfsDir/$language -I noop-carray)
    for app in $apps
    do
        [ ! -d /ssd/images/snapshot/$app-$language ] && mkdir /ssd/images/snapshot/$app-$language
        sudo tmp/bin/release/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel tmp/images/vmlinux-4.20.0 \
            --rootfs /ssd/images/rootfs/$app-$language.ext4 \
            --dump_dir /ssd/images/snapshot/$app-$language \
            --firerunner tmp/bin/release/firerunner \
            --network 'tap0/aa:bb:cc:dd:ff:00' >/dev/null
    done
done
