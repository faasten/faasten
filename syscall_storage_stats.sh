#!/usr/bin/env bash

sfclient --principal user newgate --base-dir :home:user,user --function sys-fs --gate-name sys-fs --policy user,user

#rm -r func-storage-stats
mkdir -p syscall-storage-stats
for (( i=0; i<1000; i++ ));
do
    sfclient --principal user --stat syscall-storage-stats/stat$i invoke --gate ':home:user,user:sys-fs' --server localhost:3344 < run/syscall-storage.json
done
