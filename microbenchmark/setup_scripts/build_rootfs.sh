#!/usr/bin/env bash

# only source the environment file when invoked directly from command line
if [ $(ps -o stat= -p $PPID) == 'Ss' ]; then
	source ./default_env
fi
echo 'Building app generic root filesystems...'
cd ../snapfaas-images/separate
echo "switching to $PWD"
for runtime in python3 nodejs python3-exec nodejs-exec python3-threshold
do
    echo "- $SSDROOTFSDIR/$runtime.ext4"
    ./mk_rtimage.sh $runtime $SSDROOTFSDIR/$runtime.ext4 &>/dev/null
    echo "- $MEMROOTFSDIR/$runtime.ext4"
    cp $SSDROOTFSDIR/$runtime.ext4 $MEMROOTFSDIR/$runtime.ext4
done

echo 'Building app specific root filesystems...'
cd ../combined
echo "switching to $PWD"
languages=(python3 nodejs)
appfsDir='../appfs'
for language in "${languages[@]}"
do
    apps=$(ls $appfsDir/$language)
    for app in $apps
    do
	if [ $runtime == 'python3' ]; then
	    PKGDIR=package
	fi
	if [ $runtime == 'nodejs' ]; then
	    PKGDIR=node_modules
	fi
	if [ -d $appfsDir/$runtime/$app/$PKGDIR ] && [ $(ls $appfsDir/$runtime/$app/$PKGDIR | wc -l) -eq 0 ]; then
	    rm -r $appfsDir/$runtime/$app/$PKGDIR
	fi
        echo "- $app-$language.ext4"
        ./mk_appimage.sh $language $SSDROOTFSDIR/$app-$language.ext4 $appfsDir/$language/$app &>/dev/null
    done
done

cd ../../microbenchmark
echo "swithing back to $PWD"
