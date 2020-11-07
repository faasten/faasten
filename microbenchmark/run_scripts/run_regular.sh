#!/usr/bin/env bash

if [ $# -ne 3 ]; then
    echo 'usage: run_scripts/run_regular.sh mem|nvm|ssd|hdd START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi

source ./default_env

case "$1" in
    ssd)
        rootfsdir=$SSDROOTFSDIR/regular
        ;;
    hdd)
        rootfsdir=$HDDROOTFSDIR/regular
        ;;
    mem)
        rootfsdir=$MEMROOTFSDIR/regular
        odirectFlag='--no_odirect_root'
        ;;
    nvm)
        rootfsdir=$NVMROOTFSDIR/regular
        ;;
    *)
        echo 'Error: the second positional argument must be either mem or nvm or sdd or hdd'
        exit 1
        ;;
esac

startindex=$2
endindex=$(($2 + $3 - 1))

[ $(cat ./.stat | grep setup | wc -l) -ne 1 ] && echo 'Please run ./setup.sh before run this script.' && exit 1

echo "Starting regular from $1..."
outdir=regular-$1-out
[ ! -d $outdir ] && mkdir $outdir
for ((i=$startindex; i<=$endindex; i++))
do
    echo "Round $i"
    for runtime in "${RUNTIMES[@]}"
    do
        for app in $(ls ../snapfaas-images/appfs/$runtime)
        do
            echo "- $app-$runtime"
            rootfs=$rootfsdir/$app-$runtime.ext4
            [ ! -f $rootfs ] && echo $rootfs' does not exist' && exit 1
	    cat ../resources/requests/$app-$runtime.json | head -1 | \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $rootfs \
                $odirectFlag > $outdir/$app-$runtime.$i.txt
            [ $? -ne 0 ] && echo '!! failed' && exit 1
        done
    done
done
