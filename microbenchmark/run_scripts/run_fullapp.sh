#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: run_scripts/run_diff.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi
startindex=$1
rounds=$(($1 + $2 - 1))

[ $(cat ./.stat | head -1) != 'setup' ] && echo 'Please run ./setup.sh before run this script.' && exit 1

source ./env

echo 'Starting fullapp ondemand...'
[ ! -d full-app-ondemand-out ] && mkdir full-app-ondemand-out
for ((i=$startindex; i<=$rounds; i++))
do
    echo "Round $i"
    # drop page cache
    echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
    for runtime in python3 nodejs
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
                --load_dir $SSDSNAPSHOTDIR/$app-$runtime \
                > full-app-ondemand-out/$app-$runtime.$i.txt
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done

echo 'Starting fullapp eager...'
# drop page cache
echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
[ ! -d full-app-eager-out ] && mkdir full-app-eager-out
for ((i=$startindex; i<=$rounds; i++))
do
    echo "Round $i"
    for runtime in python3 nodejs
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
                --load_dir $SSDSNAPSHOTDIR/$app-$runtime \
		--copy_base \
		--odirect_base \
                > full-app-eager-out/$app-$runtime.$i.txt
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done
