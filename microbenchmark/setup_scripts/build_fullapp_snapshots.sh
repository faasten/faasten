#!/usr/bin/env bash

source ./default_env

echo 'creating snapshot directories...'
[ ! -d $MEMSNAPSHOTDIR ] && mkdir -p $MEMSNAPSHOTDIR
#[ ! -d $NVMSNAPSHOTDIR ] && mkdir -p $NVMSNAPSHOTDIR
#[ ! -d $SSDSNAPSHOTDIR ] && mkdir -p $SSDSNAPSHOTDIR
#[ ! -d $HDDSNAPSHOTDIR ] && mkdir -p $HDDSNAPSHOTDIR

appfsDir='../snapfaas-images/appfs'
echo 'Building full-app snapshots...'
for language in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$language)
    do
        echo "- $MEMSNAPSHOTDIR/$app-$language"
        [ ! -d $MEMSNAPSHOTDIR/$app-$language ] && mkdir $MEMSNAPSHOTDIR/$app-$language
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --network $NETDEV \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --rootfs $MEMROOTFSDIR/fullapp/$app-$language.ext4 \
            --dump_dir $MEMSNAPSHOTDIR/$app-$language \
            --no_odirect_root \
            --force_exit >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1

        #echo "- $SSDSNAPSHOTDIR/$app-$language"
        #[ ! -d $SSDSNAPSHOTDIR/$app-$language ] && mkdir $SSDSNAPSHOTDIR/$app-$language
        #cp $SSDSNAPSHOTDIR/$app-$language/* $MEMSNAPSHOTDIR/$app-$language
        #echo "- $NVMSNAPSHOTDIR/$app-$language"
        #[ ! -d $NVMSNAPSHOTDIR/$app-$language ] && mkdir $NVMSNAPSHOTDIR/$app-$language
        #cp $SSDSNAPSHOTDIR/$app-$language/* $NVMSNAPSHOTDIR/$app-$language
        #echo "- $HDDSNAPSHOTDIR/$app-$language"
        #[ ! -d $HDDSNAPSHOTDIR/$app-$language ] && mkdir $HDDSNAPSHOTDIR/$app-$language
        #cp $SSDSNAPSHOTDIR/$app-$language/* $HDDSNAPSHOTDIR/$app-$language
    done
done

exit 0
