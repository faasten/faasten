#!/usr/bin/env bash

if [ $# -ne 4 ]; then
    echo 'usage: run_scripts/run_diff.sh eager|ondemand mem|memmem|nvm|ssd|hdd START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi

source ./default_env

case "$1" in
    eager)
        mode='--copy_diff'
        ;;
    ondemand)
        mode=''
        ;;
    *)
        echo 'Error: the first positional argument must be either eager or ondemand'
        exit 1
        ;;
esac

case "$2" in
    ssd)
        rootfsdir=$SSDROOTFSDIR/snapfaas
        appfsdir=$SSDAPPFSDIR
        snapshotdir=$SSDSNAPSHOTDIR
        ;;
    hdd)
        rootfsdir=$HDDROOTFSDIR/snapfaas
        appfsdir=$HDDAPPFSDIR
        snapshotdir=$HDDSNAPSHOTDIR
        ;;
    mem)
	rootfsdir=$SSDROOTFSDIR/snapfaas
	appfsdir=$SSDAPPFSDIR
	snapshotdir=$MEMSNAPSHOTDIR
        odirectFlag='--no_odirect_diff'
	;;
    memmem)
        rootfsdir=$MEMROOTFSDIR/snapfaas
        appfsdir=$MEMAPPFSDIR
        snapshotdir=$MEMSNAPSHOTDIR
        odirectFlag='--no_odirect_diff --no_odirect_root --no_odirect_app'
        ;;
    nvm)
        rootfsdir=$NVMROOTFSDIR/snapfaas
	appfsdir=$NVMAPPFSDIR
	snapshotdir=$NVMSNAPSHOTDIR
	;;
    *)
        echo 'Error: the second positional argument must be either sdd or hdd or mem or nvm'
        exit 1
        ;;
esac

startindex=$3
endindex=$(($3 + $4 - 1))

echo "Starting SNAPFASS $1 from $2..."
outdir=snapfaas-$1-$2-out
[ ! -d $outdir ] && mkdir $outdir
for ((i=$startindex; i<=$endindex; i++))
do
    echo "Round $i"
    # drop page cache
    echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
    for app in "${RUNAPPS[@]}"
    do
        echo "- $app"
        runtime=$(echo $app | grep -o '[^-]*$')
        rootfs=$rootfsdir/$runtime.ext4
        appfs=$appfsdir/$app.ext2
        snapshot=$snapshotdir/diff/$app
        [ ! -f $rootfs ] && echo $rootfs' does not exist' && exit 1
        [ ! -f $appfs ] && echo $appfs' does not exist' && exit 1
        if [ ! -d $snapshot ] || [ $(ls $snapshot | wc -l) -eq 0 ]; then
            echo $snapshot' does not exist or is empty'
            exit 1
        fi
        cat ../resources/requests/$app.json | head -1 | \
        taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel $KERNEL \
            --network $NETDEV \
            --firerunner $MEMBINDIR/firerunner \
            --rootfs $rootfs \
            --appfs $appfs \
            --load_dir $MEMSNAPSHOTDIR/$runtime,$snapshot \
            $mode $odirectFlag > $outdir/$app.$i.txt
        [ $? -ne 0 ] && echo '!! failed' && exit 1
    done
done
