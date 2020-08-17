#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: run_scripts/run_diff.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi
startindex=$1
rounds=$(($1 + $2 - 1))

[ $(cat ./.stat | head -1) != 'setup' ] && echo 'Please run ./setup.sh before run this script.' && exit 1

source ./env

echo 'Starting...'
# drop page cache
echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
source ./env
[ ! -d out ] && mkdir out
for ((i=$startindex; i<=$rounds; i++))
do
    echo "Round $i"
    for runtime in python3 nodejs
    do
        for app in $(ls ../snapfaas-images/appfs/$runtime)
        do
            echo "$app-$runtime"
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network 'tap0/aa:bb:cc:dd:ff:00' \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $SSDROOTFSDIR/$runtime.ext4 \
                --appfs $SSDAPPFSDIR/$app-$runtime.ext2 \
                --load_dir $MEMSNAPSHOTDIR/$runtime \
                --diff_dirs $SSDSNAPSHOTDIR/diff/$app-$runtime \
                --copy_diff > out/$app-$runtime.$i.txt < ../resources/requests/$app-$runtime.json
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done
