#!/usr/bin/env bash

if [ $# -ne 4 ]; then
    echo 'usage: run_scripts/run_fullapp.sh eager|ondemand mem|nvm|ssd|hdd START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi

source ./default_env

case "$1" in
    eager)
        mode='--copy_base'
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
        rootfsdir=$SSDROOTFSDIR/fullapp
        snapshotdir=$SSDSNAPSHOTDIR
        ;;
    hdd)
        rootfsdir=$HDDROOTFSDIR/fullapp
        snapshotdir=$HDDSNAPSHOTDIR
        ;;
    mem)
	rootfsdir=$SSDROOTFSDIR/fullapp
	snapshotdir=$MEMSNAPSHOTDIR
	;;
    memmem)
        rootfsdir=$MEMROOTFSDIR/fullapp
        snapshotdir=$MEMSNAPSHOTDIR
        odirectFlag='--no_odirect_root'
        ;;
    nvm)
        rootfsdir=$NVMROOTFSDIR/fullapp
	snapshotdir=$NVMSNAPSHOTDIR
	;;
    *)
        echo 'Error: the second positional argument must be either sdd or hdd or mem or nvm or memmem'
        exit 1
        ;;
esac

startindex=$3
endindex=$(($3 + $4 - 1))

echo "Starting REAP $1 from $2..."
outdir=reap-$1-$2-out
[ ! -d $outdir ] && mkdir $outdir
for ((i=$startindex; i<=$endindex; i++))
do
    echo "Round $i"
    # drop page cache
    echo 1 | sudo tee /proc/sys/vm/drop_caches &>/dev/null
    for runtime in "${RUNTIMES[@]}"
    do
        for app in $(ls ../snapfaas-images/appfs/$runtime)
        do
            echo "- $app-$runtime"
            rootfs=$rootfsdir/$app-$runtime.ext4
            snapshot=$snapshotdir/$app-$runtime
            [ ! -f $rootfs ] && echo $rootfs' does not exist' && exit 1
            if [ ! -d $snapshot ] || [ $(ls $snapshot | wc -l) -eq 0 ]; then
                echo $snapshot' does not exist or is empty'
                exit 1
            fi
	    cat ../resources/requests/$app-$runtime.json | head -1 | \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $rootfs \
                --load_dir $snapshot \
                --load_ws \
                $mode $odirectFlag > $outdir/$app-$runtime.$i.txt
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done
