#!/bin/bash


if [ $# -ne 1 ]; then
    echo 'usage: SCRIPT NUMBER_OF_ROUNDS'
    exit 1
fi
rounds=$1

[ ! -d microbenchmark/out ] && mkdir microbenchmark/out
for ((i=0; i<$rounds; i++))
do
    # drop page cache
    echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
    cat tmp/bin/release/firerunner >/dev/null
    cat tmp/bin/release/fc_wrapper >/dev/null
    cat tmp/images/vmlinux-4.20.0 >/dev/null
    cat tmp/images/python3.ext4 >/dev/null
    cat tmp/images/snapshot/python3/* >/dev/null
    for app in $(dir /ssd/images/snapshot/diff)
    do
        taskset -c 0 sudo tmp/bin/release/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel tmp/images/vmlinux-4.20.0 \
            --rootfs tmp/images/python3.ext4 \
            --appfs /ssd/images/appfs/$app.ext2 \
            --load_dir tmp/images/snapshot/python3 \
            --diff_dirs /ssd/images/snapshot/diff/$app \
            --firerunner tmp/bin/release/firerunner \
            --network 'tap0/aa:bb:cc:dd:ff:00' \
            --copy_diff > microbenchmark/out/$app.$i.txt < microbenchmark/requests/$app.json
        if [ $? -eq 1 ]; then
            echo 'error: failed to execute' $app
            exit 1
        fi
    done
done
