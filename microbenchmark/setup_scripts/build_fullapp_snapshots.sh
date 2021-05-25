#!/usr/bin/env bash

source ./default_env

echo 'creating snapshot directories...'
[ ! -d $MEMSNAPSHOTDIR ] && mkdir -p $MEMSNAPSHOTDIR

appfsDir=../snapfaas-images/appfs
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
    done
done

echo 'Building WS...'
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
            --load_dir $MEMSNAPSHOTDIR/$app-$language \
            --dump_ws \
            --no_odirect_root \
            < ../resources/requests/$app-$language.json >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1
    done
done
exit 0
