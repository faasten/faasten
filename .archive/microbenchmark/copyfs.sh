#!/usr/bin/env bash

if [ $# -ne 1 ]; then
    echo 'usage: ./copyfs.sh TARGET_DIRECTORY'
    exit 1
fi

source ./default_env
targetDir=$1
case "$targetDir" in
    $NVMROOT)
        ;;
    $SSDROOT)
        ;;
    $HDDROOT)
        ;;
    *)
        echo 'Error: the target directory must be nvm|ssd|hdd root directory defined in default_env'
        exit 1
        ;;
esac
# check the existence of target directory
if [ ! -d $targetDir ]; then
    echo "$targetDir does not exist"
    exit 1
fi

mountpoint -q $targetDir
if [ $? -eq 1 ]; then
    echo "INFO: $targetDir is not a mountpoint"
    echo "INFO: root device rotational=$(lsblk -o mountpoint,rota | egrep "^/ +" | awk '{print $2}')"
else 
    rotational=$(lsblk -o rota,mountpoint | egrep $targetDir | awk '{print $1}')
    device=$(lsblk -o kname,mountpoint | egrep $targetDir | awk '{print $1}')
    echo "INFO: device $device (rotational=$rotational) is mounted at $targetDir"
fi
sudo chown -R $(id -un):$(id -gn) $targetDir

cp -r $MEMROOT/rootfs $targetDir
cp -r $MEMROOT/appfs $targetDir
cp -r $MEMROOT/snapshots $targetDir
