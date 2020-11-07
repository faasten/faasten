#!/usr/bin/env bash

# check docker can run in non-root mode
echo 'checking docker can run in non-root mode...'
docker run hello-world &>/dev/null
if [ $? -ne 0 ]; then
	echo 'Check that docker daemon is running using `service docker status`.'
	echo 'If docker daemon is not running, run `sudo service docker start` to start it.'
	echo 'If docker daemon is running, docker currently cannot run in non-root mode. Use `sudo usermod -aG docker $USER` to make it runnable in non-root mode.'
	exit 1
fi

source ./default_env
echo $MEMROOT
echo "mounting 20GB tmpfs at $MEMROOT..."
# mount a 20GB tmpfs at /tmp/snapfaas
[ ! -d $MEMROOT ] && mkdir $MEMROOT
mountpoint -q $MEMROOT
[ $? -eq 1 ] && sudo mount -t tmpfs -o size=20G tmpfs $MEMROOT

echo "copying kernel image to $MEMROOT/kernel..."
[ ! -d $MEMROOT/kernel ] && mkdir $MEMROOT/kernel
cp ../resources/images/vmlinux-4.20.0 $MEMROOT/kernel

# deploy door device if one is not deployed
if [ $(docker ps | grep door | wc -l) -ne 1 ]; then
	echo 'Deploying alexa-door device...'
	full_path=$(dirname $(realpath $0))
	cd ../snapfaas-images/appfs/nodejs/alexa-door/door-device
	echo "Switching to directory $PWD..."
	./deploy.sh
	cd $full_path
	echo "Switching to directory $PWD..."
fi

# configure tap0 if it does not exists
if [ $(ifconfig | grep tap0 | wc -l) -ne 1 ]; then
	echo 'Configuring tap0...'
	../scripts/setup-tap-bridge.sh 1
fi

# prerequisites for building snapshots
prereq=1
[ ! -f .stat ] && touch .stat
# build root filesystems
if [ $(cat .stat | grep 'rootfs' | wc -l) -eq 0 ]; then
	setup_scripts/build_rootfs.sh
	if [ $? -ne 0 ]; then
		tput setaf 1; echo 'Building root filesystems failed'
		tput sgr0
		prereq=0
	else
		echo 'rootfs' >> .stat
	fi
fi
# build app filesystems
if [ $(cat .stat | grep 'appfs' | wc -l) -eq 0 ]; then
	setup_scripts/build_appfs.sh
	if [ $? -ne 0 ]; then
		tput setaf 1; echo 'Building app filesystems failed'
		tput sgr0
		prereq=0
	else
		echo 'appfs' >> .stat
	fi
fi
# build firerunner/fc_wrapper binaries
[ ! -d $MEMBINDIR ] && mkdir -p $MEMBINDIR
if [ $(cat .stat | grep 'binaries' | wc -l) -eq 0 ]; then
	setup_scripts/build_binaries.sh
	if [ $? -ne 0 ]; then
		tput setaf 1; echo 'Building binaries failed'
		tput sgr0
		prereq=0
	fi
	echo 'binaries' >> .stat
fi
# only proceed here when all prerequisites are successfully built
if [ $prereq -eq 1 ]; then
    complete=1
    # build language base snapshots + app diff snapshots
    if [ $(cat .stat | grep "\<diff\>" | wc -l) -eq 0 ]; then
    	setup_scripts/build_diff_snapshots.sh
    	if [ $? -ne 0 ]; then
    	    tput setaf 1; echo 'Building base+diff snapshots failed'
    	    tput sgr0
            complete=0
    	else
    	    echo 'diff' >> .stat
    	fi
    fi
    # build full-app snapshots
    if [ $(cat .stat | grep 'fullapp' | wc -l) -eq 0 ]; then
    	setup_scripts/build_fullapp_snapshots.sh
    	if [ $? -ne 0 ]; then
    	    tput setaf 1; echo 'Building fullapp snapshots failed'
    	    tput sgr0
	    complete=0
    	else
    	    echo 'fullapp' >> .stat
    	fi
    fi
    ## build diff snapshots with 'console=ttyS0' boot command line argument
    #if [ $(cat .stat | grep 'debug_diff' | wc -l) -eq 0 ]; then
    #	setup_scripts/build_debug_diff_snapshots.sh
    #	if [ $? -ne 0 ]; then
    #	    tput setaf 1; echo 'Building debug diff snapshots failed'
    #	    tput sgr0
    #        complete=0
    #	else
    #	    echo 'debug_diff' >> .stat
    #	fi
    #fi
    [ $complete -eq 1 ] && echo 'setup' >> .stat
fi
