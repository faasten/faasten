#!/usr/bin/env bash

if [ $# -ne 4 ]; then
    echo 'usage: run_scripts/run_diff.sh eager|ondemand mem|nvm|ssd|hdd START_INDEX NUMBER_OF_ROUNDS'
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
    nvm)
        rootfsdir=$SSDROOTFSDIR/snapfaas
	appfsdir=$SSDAPPFSDIR
	snapshotidr=$NVMSNAPSHOTDIR
	;;
    *)
        echo 'Error: the second positional argument must be either sdd or hdd or mem or nvm'
        exit 1
        ;;
esac

startindex=$3
endindex=$(($3 + $4 - 1))

[ $(cat ./.stat | grep setup | wc -l) -ne 1 ] && echo 'Please run ./setup.sh before run this script.' && exit 1

echo "Starting snapfaas $1 from $2..."
outdir=snapfaas-$1-$2-out
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
            echo "$app-$runtime"
	    cat ../resources/requests/$app-$runtime.json | head -1 | \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $rootfsdir/$runtime.ext4 \
                --appfs $appfsdir/$app-$runtime.ext2 \
                --load_dir $MEMSNAPSHOTDIR/$runtime \
                --diff_dirs $snapshotdir/diff/$app-$runtime \
                $mode $odirectFlag > $outdir/$app-$runtime.$i.txt
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done
