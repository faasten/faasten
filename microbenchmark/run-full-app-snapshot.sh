#!/bin/bash

if [ ! -d microbenchmark ] || [ ! -d resources ]; then
    echo 'Directory microbenchmark and resources do not exsit in current working directory'
    exit 1
fi

if [ $# -ne 1 ]; then
    echo 'usage: microbenchmark/run-full-app-snapshot.sh NUMBER_OF_ROUNDS'
    exit 1
fi
rounds=$1

[ ! -d microbenchmark/full-app-out ] && mkdir microbenchmark/full-app-out
for ((i=0; i<$rounds; i++))
do
    # drop page cache
    echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
    cat tmp/bin/release/firerunner >/dev/null
    cat tmp/bin/release/fc_wrapper >/dev/null
    cat tmp/images/vmlinux-4.20.0 >/dev/null
    for app in $(ls /ssd/images/snapshot -I python3 -I nodejs -I diff)
    do
        taskset -c 0 sudo tmp/bin/release/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel tmp/images/vmlinux-4.20.0 \
            --rootfs /ssd/images/rootfs/$app.ext4 \
            --load_dir /ssd/images/snapshot/$app \
            --firerunner tmp/bin/release/firerunner \
            --network 'tap0/aa:bb:cc:dd:ff:00' > microbenchmark/full-app-out/$app.$i.txt < resources/requests/$app.json
        if [ $? -ne 0 ]; then
            echo 'error: failed to execute' $app
            exit 1
        fi
    done
done

[ ! -d microbenchmark/regular-out ] && mkdir microbenchmark/regular-out
for ((i=0; i<$rounds; i++))
do
    # drop page cache
    echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
    cat tmp/bin/release/firerunner >/dev/null
    cat tmp/bin/release/fc_wrapper >/dev/null
    cat tmp/images/vmlinux-4.20.0 >/dev/null
    for app in $(ls /ssd/images/snapshot -I python3 -I diff -I nodejs)
    do
        taskset -c 0 sudo tmp/bin/release/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel tmp/images/vmlinux-4.20.0 \
            --rootfs /ssd/images/rootfs/$app.ext4 \
            --firerunner tmp/bin/release/firerunner \
            --network 'tap0/aa:bb:cc:dd:ff:00'  > microbenchmark/regular-out/$app.$i.txt < resources/requests/$app.json
        if [ $? -ne 0 ]; then
            echo 'error: failed to execute' $app
            exit 1
        fi
    done
done
