#!/usr/bin/env bash

source ./default_env

echo 'creating snapshot directories...'
[ ! -d $MEMSNAPSHOTDIR ] && mkdir -p $MEMSNAPSHOTDIR
[ ! -d $SSDSNAPSHOTDIR ] && mkdir -p $SSDSNAPSHOTDIR
[ ! -d $HDDSNAPSHOTDIR ] && mkdir -p $HDDSNAPSHOTDIR

appfsDir='../snapfaas-images/appfs'
echo 'Building full-app snapshots...'
for language in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$language)
    do
        echo "- $SSDSNAPSHOTDIR/$app-$language"
        [ ! -d $SSDSNAPSHOTDIR/$app-$language ] && mkdir $SSDSNAPSHOTDIR/$app-$language
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --network $NETDEV \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --rootfs $SSDROOTFSDIR/fullapp/$app-$language.ext4 \
            --dump_dir $SSDSNAPSHOTDIR/$app-$language \
            --force_exit >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1

        echo "- $MEMSNAPSHOTDIR/$app-$language"
        [ ! -d $MEMSNAPSHOTDIR/$app-$language ] && mkdir $MEMSNAPSHOTDIR/$app-$language
        cp $SSDSNAPSHOTDIR/$app-$language/* $MEMSNAPSHOTDIR/$app-$language
        echo "- $HDDSNAPSHOTDIR/$app-$language"
        [ ! -d $HDDSNAPSHOTDIR/$app-$language ] && mkdir $HDDSNAPSHOTDIR/$app-$language
        cp $SSDSNAPSHOTDIR/$app-$language/* $HDDSNAPSHOTDIR/$app-$language
    done
done
