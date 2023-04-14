#!/usr/bin/env bash

if [ -z "${KERNEL}" ]; then
    echo 'KERNEL is unset'
    exit 1
fi

if [ -z "${PYTHON}" ]; then
    echo 'PYTHON is unset'
    exit 1
fi

if [ -z "${FSUTIL}" ]; then
    echo 'FSUTIL is unset'
    exit 1
fi

tikv=$1

ops=(create-dir read-dir delete-dir create-faceted read-faceted delete-faceted create-file write read-file delete-file label-taint label-endorse label-declassify gen-blob)

rm -r stats
rm -r output

# setup output dir per op
for op in "${ops[@]}"; do
    mkdir -p stats/$op
    mkdir -p output/$op
done

dd if=/dev/urandom of=4K bs=4K count=1 &>/dev/null
for (( i=0; i<100; i++ )); do
    for op in "${ops[@]}"; do
        cat jsons/$op | singlevm $tikv --kernel $KERNEL --rootfs $PYTHON --appfs $FSUTIL \
        --login faasten --stats stats/$op/stat$i \
        > output/$op/output$i
    done
done
