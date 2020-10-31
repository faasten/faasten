#!/usr/bin/env bash

source ./default_env

echo 'creating appfs directories...'
[ ! -d $MEMAPPFSDIR ] && mkdir -p $MEMAPPFSDIR
[ ! -d $SSDAPPFSDIR ] && mkdir -p $SSDAPPFSDIR
[ ! -d $HDDAPPFSDIR ] && mkdir -p $HDDAPPFSDIR

echo $RUNTIMES
echo $KERNEL
echo 'Building appfs...'
appfsDir='../snapfaas-images/appfs'
for runtime in "${RUNTIMES[@]}"
do
    for app in $(ls $appfsDir/$runtime)
    do
	# make sure dependencies are installed when an empty package/node_modules directory exists
	# which can occur due to a failed build.
	if [ $runtime == 'python3' ]; then
	    pkgdir=package
	fi
	if [ $runtime == 'nodejs' ]; then
	    pkgdir=node_modules
	fi
	if [ -d $appfsDir/$runtime/$app/$pkgdir ] && [ $(ls $appfsDir/$runtime/$app/$pkgdir | wc -l) -eq 0 ]; then
		rm -r $appfsDir/$runtime/$app/$pkgdir
	fi
        make -C $appfsDir/$runtime/$app &>/dev/null
        echo "- $SSDAPPFSDIR/$app-$runtime.ext2"
        cp $appfsDir/$runtime/$app/output.ext2 $SSDAPPFSDIR/$app-$runtime.ext2
        echo "- $HDDAPPFSDIR/$app-$runtime.ext2"
        cp $appfsDir/$runtime/$app/output.ext2 $HDDAPPFSDIR/$app-$runtime.ext2
        echo "- $MEMAPPFSDIR/$app-$runtime.ext2"
        mv $appfsDir/$runtime/$app/output.ext2 $MEMAPPFSDIR/$app-$runtime.ext2
    done
done
