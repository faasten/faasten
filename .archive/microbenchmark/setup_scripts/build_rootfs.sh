#!/usr/bin/env bash

source ./default_env

echo 'creating rootfs directories...'
for version in fullapp snapfaas
do
    [ ! -d $MEMROOTFSDIR/$version ] && mkdir -p $MEMROOTFSDIR/$version
done


echo 'Building snapfaas root filesystems...'
cd ../snapfaas-images/rootfs/snapfaas
echo "switching to $PWD"
for runtime in "${RUNTIMES[@]}"
do
    echo "- $MEMROOTFSDIR/snapfaas/$runtime.ext4"
    ./mk_rtimage.sh $runtime $MEMROOTFSDIR/snapfaas/$runtime.ext4 &>/dev/null
done

for version in fullapp
do
    echo "Building $version root filesystems..."
    cd ../$version
    echo "switching to $PWD"
    appfsDir='../../appfs'
    for runtime in "${RUNTIMES[@]}"
    do
        apps=$(ls $appfsDir/$runtime)
        for app in $apps
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
            echo "- $MEMROOTFSDIR/$version/$app-$runtime.ext4"
            ./mk_appimage.sh $runtime $appfsDir/$runtime/$app $MEMROOTFSDIR/$version/$app-$runtime.ext4 &>/dev/null
        done
    done
done

cd ../../../microbenchmark
echo "swithing back to $PWD"
