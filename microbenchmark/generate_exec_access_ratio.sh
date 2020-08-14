#!/bin/bash

if [ $(dir $PWD | grep snapfaas-images | wc -l) -ne 1 ] && \
    [ $(dir $PWD | grep tmp | wc -l) -ne 1 ]; then
    echo "snapfaas-images and tmp does not exist in $PWD"
    exit 1
fi

if [ ! -d /ssd ]; then
    echo '/ssd must exist'
    exit 1
fi

ROOTFS=tmp/images
APPFS=snapfaas-images/appfs
EXECSNAPSHOT=/ssd/images/exec-snapshot
[ ! -d $EXECSNAPSHOT ] && mkdir -p /ssd/images/exec-snapshot
runtimes=( python3 nodejs )
echo 'Generating language snapshots...'
for runtime in "${runtimes[@]}"
do
    [ ! -d $EXECSNAPSHOT/$runtime ] && mkdir $EXECSNAPSHOT/$runtime
    cd snapfaas-images/separate
    ./mk_rtimage.sh $runtime-exec ../../$ROOTFS/$runtime-exec.ext4 >/dev/null
    cd ../..
    sudo tmp/bin/release/fc_wrapper \
        --kernel resources/images/vmlinux-4.20.0 \
        --rootfs $ROOTFS/$runtime-exec.ext4 \
        --appfs $APPFS/empty/output.ext2 \
        --network 'tap0/aa:bb:cc:dd:ff:ff' \
        --mem_size 128 \
        --vcpu_count 1 \
        --dump_dir $EXECSNAPSHOT/$runtime \
        --firerunner tmp/bin/release/firerunner \
        --force_exit >/dev/null
done

echo 'Generating diff snapshots...'
for runtime in "${runtimes[@]}"
do
    for app in $(dir $APPFS/$runtime)
    do
        if [[ $app =~ ^\. ]]; then
            continue
        fi
        [ ! -f $APPFS/$runtime/$app/output.ext2 ] && make -C $APPFS/$runtime/$app
        [ ! -d $EXECSNAPSHOT/diff/$app-$runtime-func ] && mkdir -p $EXECSNAPSHOT/diff/$app-$runtime-func
        [ ! -d $EXECSNAPSHOT/diff/$app-$runtime-exec ] && mkdir -p $EXECSNAPSHOT/diff/$app-$runtime-exec
        echo $app-$runtime
        sudo tmp/bin/release/fc_wrapper \
            --kernel resources/images/vmlinux-4.20.0 \
            --rootfs $ROOTFS/$runtime-exec.ext4 \
            --appfs $APPFS/$runtime/$app/output.ext2 \
            --network 'tap0/aa:bb:cc:dd:ff:ff' \
            --mem_size 128 \
            --vcpu_count 1 \
            --load_dir $EXECSNAPSHOT/$runtime \
            --firerunner tmp/bin/release/firerunner \
            --dump_dir $EXECSNAPSHOT/diff/$app-$runtime-func \
            --force_exit >/dev/null

        sudo tmp/bin/release/fc_wrapper \
            --kernel resources/images/vmlinux-4.20.0 \
            --rootfs $ROOTFS/$runtime-exec.ext4 \
            --appfs $APPFS/$runtime/$app/output.ext2 \
            --network 'tap0/aa:bb:cc:dd:ff:ff' \
            --mem_size 128 \
            --vcpu_count 1 \
            --load_dir $EXECSNAPSHOT/$runtime \
            --firerunner tmp/bin/release/firerunner \
            --diff_dirs $EXECSNAPSHOT/diff/$app-$runtime-func \
            --dump_dir $EXECSNAPSHOT/diff/$app-$runtime-exec < resources/requests/$app-$runtime.json >/dev/null
    done
done

>microbenchmark/ratio.txt
for runtime in "${runtimes[@]}"
do
    for app in $(dir $APPFS/$runtime)
    do
        echo $app-$runtime >> microbenchmark/ratio.txt
        snapfaas-images/python_scripts/parse_page_numbers.py $EXECSNAPSHOT/$runtime/page_numbers,$EXECSNAPSHOT/diff/$app-$runtime-func/page_numbers,$EXECSNAPSHOT/diff/$app-$runtime-exec/page_numbers >> microbenchmark/ratio.txt
    done
done
