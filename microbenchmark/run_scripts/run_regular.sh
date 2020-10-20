#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: run_scripts/run_diff.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi
startindex=$1
rounds=$(($1 + $2 - 1))

[ $(cat ./.stat | grep setup | wc -l) -ne 1 ] && echo 'Please run ./setup.sh before run this script.' && exit 1

source ./default_env

echo 'Starting...'
# drop page cache
echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
[ ! -d regular-out ] && mkdir regular-out
for ((i=$startindex; i<=$rounds; i++))
do
    echo "Round $i"
    for runtime in "${RUNTIMES[@]}"
    do
        for app in $(ls ../snapfaas-images/appfs/$runtime)
        do
            echo "- $app-$runtime"
	    cat ../resources/requests/$app-$runtime.json | head -1 | \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network 'tap0/aa:bb:cc:dd:ff:00' \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $SSDROOTFSDIR/$app-$runtime.ext4 \
                > regular-out/$app-$runtime.$i.txt
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done
