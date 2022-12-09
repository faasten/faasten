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
