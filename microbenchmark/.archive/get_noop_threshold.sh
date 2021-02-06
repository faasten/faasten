#!/usr/bin/env bash

if [ ! -d snapfaas-images ]; then
    echo 'snapfaas-images does not exist in current directory'
    exit 1
fi

echo 'Building noop filesystems...'
for (( i=0; i<=10; i++ ))
do
    make -C snapfaas-images/appfs/python3/.noop-carray/noop-$im >/dev/null
done

#####################################################################
#            Noop/python3-noop Diff Snapshots Generation            #
#####################################################################
# in memory base snapshot
BASESNAPSHOTDIR=tmp/images/exec-snapshot
FCWRAPPER=
FIRERUNNER=
echo 'Building noop diff snapshots...'
for (( i=0; i<=10; i++ ))
do
    tasket -c 0 sudo $FCWRAPPER \
        --vcpu_count 1 --mem_size 128 \
        --kernel tmp/images/vmlinux-4.20.0 \
        --firerunner tmp/bin/release/firerunner \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --rootfs $ROOTFSDIR/python3.ext4 \
        --appfs $APPFSDIR/noop-"$i"m \
        --load_dir $BASESNAPSHOTDIR/python3 \
        --dump_dir $DIFFSNAPSHOTDIR/noop-"i"m \
        --force_exit
    # get access ratio
    snapfaas-images/python_scripts/parse_page_numbers.py \
        $BASE_SNAPSHOT_DIR/python3/page_numbers
done

echo 'Building python3-noop diff snapshots...'
FCWRAPPER=
FIRERUNNER=
for (( i=0; i<=10; i++ ))
do
    tasket -c 0 sudo $FCWRAPPER \
        --vcpu_count 1 --mem_size 128 \
        --kernel tmp/images/vmlinux-4.20.0 \
        --firerunner tmp/bin/release/firerunner \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --rootfs $ROOTFSDIR/python3.ext4 \
        --appfs $APPFSDIR/noop-"$i"m \
        --load_dir $BASESNAPSHOTDIR/python3 \
        --dump_dir $DIFFSNAPSHOTDIR/noop-"i"m \
        --force_exit
    snapfaas-images/python_scripts/parse_page_numbers.py \
        $BASE_SNAPSHOT_DIR/kernel/page_numbers
done

#####################################################################
#            Running Noop/python3-noop                              #
#####################################################################
echo 'running noop from noop diff snapshots...'
echo '+++++on demand'
for (( i=0; i<=10; i++ ))
do
    tasket -c 0 sudo $FCWRAPPER \
        --vcpu_count 1 --mem_size 128 \
        --kernel tmp/images/vmlinux-4.20.0 \
        --firerunner tmp/bin/release/firerunner \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --rootfs $ROOTFSDIR/python3.ext4 \
        --appfs $APPFSDIR/noop-"$i"m \
        --load_dir $BASESNAPSHOTDIR/python3 \
        --dump_dir $DIFFSNAPSHOTDIR/noop-"i"m \
        --force_exit
done

echo '+++++eager'
for (( i=0; i<=10; i++ ))
do
    tasket -c 0 sudo $FCWRAPPER \
        --vcpu_count 1 --mem_size 128 \
        --kernel tmp/images/vmlinux-4.20.0 \
        --firerunner tmp/bin/release/firerunner \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --rootfs $ROOTFSDIR/python3.ext4 \
        --appfs $APPFSDIR/noop-"$i"m \
        --load_dir $BASESNAPSHOTDIR/python3 \
        --dump_dir $DIFFSNAPSHOTDIR/noop-"i"m \
        --force_exit
done

echo 'running noop from python3-noop diff snapshots...'
echo '+++++on demand'
for (( i=0; i<=10; i++ ))
do
    tasket -c 0 sudo $FCWRAPPER \
        --vcpu_count 1 --mem_size 128 \
        --kernel tmp/images/vmlinux-4.20.0 \
        --firerunner tmp/bin/release/firerunner \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --rootfs $ROOTFSDIR/python3.ext4 \
        --appfs $APPFSDIR/noop-"$i"m \
        --load_dir $BASESNAPSHOTDIR/python3 \
        --dump_dir $DIFFSNAPSHOTDIR/noop-"i"m \
        --force_exit
done

echo '+++++eager'
for (( i=0; i<=10; i++ ))
do
    tasket -c 0 sudo $FCWRAPPER \
        --vcpu_count 1 --mem_size 128 \
        --kernel tmp/images/vmlinux-4.20.0 \
        --firerunner tmp/bin/release/firerunner \
        --network 'tap0/aa:bb:cc:dd:ff:00' \
        --rootfs $ROOTFSDIR/python3.ext4 \
        --appfs $APPFSDIR/noop-"$i"m \
        --load_dir $BASESNAPSHOTDIR/python3 \
        --dump_dir $DIFFSNAPSHOTDIR/noop-"i"m \
        --force_exit
done
