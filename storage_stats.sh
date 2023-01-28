#!/usr/bin/env bash

rm -r storage-stats
mkdir -p storage-stats/lsroot
for (( i=0; i<1000; i++ ));
do 
    sfclient --principal user1 --stat storage-stats/lsroot/stat$i ls : 1>/dev/null
done

mkdir -p storage-stats/createuserfile
mkdir -p storage-stats/writeuserfile
mkdir -p storage-stats/readuserfile
mkdir -p storage-stats/deleteuserfile
dd if=/dev/urandom of=storage-stats/1K bs=1K count=1 &>/dev/null
for (( i=0; i<1000; i++ ));
do 
    sfclient --principal user1 --stat storage-stats/createuserfile/stat$i create file --base-dir ':home:user1,user1' --name file1 --label user1,user1 1>/dev/null
    sfclient --principal user1 --stat storage-stats/writeuserfile/stat$i write ':home:user1,user1:file1' < storage-stats/1K 1>/dev/null
    sfclient --principal user1 --stat storage-stats/readuserfile/stat$i read ':home:user1,user1:file1' 1>/dev/null
    sfclient --principal user1 --stat storage-stats/deleteuserfile/stat$i del --base-dir ':home:user1,user1' --name file1 1>/dev/null
done
