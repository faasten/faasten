#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: ./run_hello_fullapp_mem.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi
startindex=$1
rounds=$(($1 + $2 - 1))

[ $(cat ./.stat | head -1) != 'setup' ] && echo 'Please run ./setup.sh before run this script.' && exit 1

source ./env

outDir=hello-fullapp-mem-ondemand-out
echo 'Starting fullapp mem ondemand...'
[ ! -d $outDir ] && mkdir $outDir
for ((i=$startindex; i<=$rounds; i++))
do
    echo "Round $i"
    for runtime in python3
    do
        for app in hello
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
                --load_dir $MEMSNAPSHOTDIR/$app-$runtime \
                > $outDir/$app-$runtime.$i.txt
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done

echo 'Starting fullapp mem eager...'
outDir=hello-fullapp-mem-eager-out
[ ! -d $outDir ] && mkdir $outDir
for ((i=$startindex; i<=$rounds; i++))
do
    echo "Round $i"
    for runtime in python3
    do
        for app in hello
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
                --load_dir $MEMSNAPSHOTDIR/$app-$runtime \
		--copy_base \
                > $outDir/$app-$runtime.$i.txt
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done
