#!/usr/bin/env bash

source ./default_env

echo 'creating snapshot directories...'
[ ! -d $MEMSNAPSHOTDIR ] && mkdir -p $MEMSNAPSHOTDIR/diff
#[ ! -d $NVMSNAPSHOTDIR ] && mkdir -p $NVMSNAPSHOTDIR/diff
#[ ! -d $SSDSNAPSHOTDIR ] && mkdir -p $SSDSNAPSHOTDIR/diff
#[ ! -d $HDDSNAPSHOTDIR ] && mkdir -p $HDDSNAPSHOTDIR/diff

echo 'Building base snapshots...'
make -C ../snapfaas-images/appfs/empty &>/dev/null
for runtime in "${RUNTIMES[@]}"
do
    echo "- $MEMSNAPSHOTDIR/$runtime"
    [ ! -d $MEMSNAPSHOTDIR/$runtime ] && mkdir $MEMSNAPSHOTDIR/$runtime
    sudo $MEMBINDIR/fc_wrapper \
        --vcpu_count 1 \
        --mem_size 128 \
        --kernel $KERNEL \
        --network $NETDEV \
        --firerunner $MEMBINDIR/firerunner \
        --rootfs $MEMROOTFSDIR/snapfaas/$runtime.ext4 \
        --appfs ../snapfaas-images/appfs/empty/output.ext2 \
        --dump_dir $MEMSNAPSHOTDIR/$runtime \
        --no_odirect_root \
        --force_exit >/dev/null
    [ $? -ne 0 ] && echo '!! failed' && exit 1

    #echo "- $SSDSNAPSHOTDIR/$runtime"
    #[ ! -d $SSDSNAPSHOTDIR/$runtime ] && mkdir $SSDSNAPSHOTDIR/$runtime
    #cp $MEMSNAPSHOTDIR/$runtime/* $SSDSNAPSHOTDIR/$runtime
    #echo "- $NVMSNAPSHOTDIR/$runtime"
    #[ ! -d $NVMSNAPSHOTDIR/$runtime ] && mkdir $NVMSNAPSHOTDIR/$runtime
    #cp $MEMSNAPSHOTDIR/$runtime/* $NVMSNAPSHOTDIR/$runtime
    #echo "- $HDDSNAPSHOTDIR/$runtime"
    #[ ! -d $HDDSNAPSHOTDIR/$runtime ] && mkdir $HDDSNAPSHOTDIR/$runtime
    #cp $MEMSNAPSHOTDIR/$runtime/* $HDDSNAPSHOTDIR/$runtime
done

echo 'Building diff snapshots...'
appfsDir=../snapfaas-images/appfs
for runtime in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$runtime)
    do
        echo "- $MEMSNAPSHOTDIR/diff/$app-$runtime"
        [ ! -d $MEMSNAPSHOTDIR/diff/$app-$runtime ] && mkdir $MEMSNAPSHOTDIR/diff/$app-$runtime
        sudo $MEMBINDIR/fc_wrapper \
            --vcpu_count 1 \
            --mem_size 128 \
            --kernel $KERNEL \
            --firerunner $MEMBINDIR/firerunner \
            --network $NETDEV \
            --rootfs $MEMROOTFSDIR/snapfaas/$runtime.ext4 \
            --appfs $MEMAPPFSDIR/$app-$runtime.ext2 \
            --load_dir $MEMSNAPSHOTDIR/$runtime \
            --dump_dir $MEMSNAPSHOTDIR/diff/$app-$runtime \
            --no_odirect_root --no_odirect_app --no_odirect_diff \
            --force_exit >/dev/null
        [ $? -ne 0 ] && echo '!! failed' && exit 1

        #echo "- $SSDSNAPSHOTDIR/diff/$app-$runtime"
        #[ ! -d $SSDSNAPSHOTDIR/diff/$app-$runtime ] && mkdir -p $SSDSNAPSHOTDIR/diff/$app-$runtime
        #cp $MEMSNAPSHOTDIR/diff/$app-$runtime/* $SSDSNAPSHOTDIR/diff/$app-$runtime
        #echo "- $NVMSNAPSHOTDIR/diff/$app-$runtime"
        #[ ! -d $NVMSNAPSHOTDIR/diff/$app-$runtime ] && mkdir $NVMSNAPSHOTDIR/diff/$app-$runtime
        #cp $MEMSNAPSHOTDIR/diff/$app-$runtime/* $NVMSNAPSHOTDIR/diff/$app-$runtime
        #echo "- $HDDSNAPSHOTDIR/diff/$app-$runtime"
        #[ ! -d $HDDSNAPSHOTDIR/diff/$app-$runtime ] && mkdir $HDDSNAPSHOTDIR/diff/$app-$runtime
        #cp $MEMSNAPSHOTDIR/diff/$app-$runtime/* $HDDSNAPSHOTDIR/diff/$app-$runtime
    done
done

exit 0
