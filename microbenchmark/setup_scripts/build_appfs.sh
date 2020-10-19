#!/usr/bin/env bash

# only source the environment file when invoked directly from command line
if [ $(ps -o stat= -p $PPID) == 'Ss' ]; then
	source ./default_env
fi
echo 'Building appfs...'
appfsDir='../snapfaas-images/appfs'
runtimes=( python3 nodejs )
for runtime in "${runtimes[@]}"
do
    for app in $(ls $appfsDir/$runtime)
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
        make -C $appfsDir/$runtime/$app &>/dev/null
        cp $appfsDir/$runtime/$app/output.ext2 $SSDAPPFSDIR/$app-$runtime.ext2
        echo "- $SSDAPPFSDIR/$app-$runtime.ext2"
        mv $appfsDir/$runtime/$app/output.ext2 $MEMAPPFSDIR/$app-$runtime.ext2
        echo "- $MEMAPPFSDIR/$app-$runtime.ext2"
    done
done
