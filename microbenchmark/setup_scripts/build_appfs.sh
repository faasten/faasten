#!/usr/bin/env bash

source ./env
echo 'Building appfs...'
appfsDir='../snapfaas-images/appfs'
runtimes=( python3 nodejs )
for runtime in "${runtimes[@]}"
do
    for app in $(ls $appfsDir/$runtime)
    do
        make -C $appfsDir/$runtime/$app &>/dev/null
        cp $appfsDir/$runtime/$app/output.ext2 $SSDAPPFSDIR/$app-$runtime.ext2
        echo "- $SSDAPPFSDIR/$app-$runtime.ext2"
        mv $appfsDir/$runtime/$app/output.ext2 $MEMAPPFSDIR/$app-$runtime.ext2
        echo "- $MEMAPPFSDIR/$app-$runtime.ext2"
    done
done
