#!/bin/sh
if [ $# -ne 1 ]; then
    echo 'usage: ./setup-tap-bridge.sh NUMBER_OF_TAPS'
    exit 1
fi
MAXTAPID=$(($1-1))
for ((i=0; i<=$MAXTAPID; i++))
do
    TAP="tap$i"
    # The first step on the host is to create a `tap` device:
    sudo ip tuntap add $TAP mode tap
    # connect tap devices to `docker0` bridge
    sudo brctl addif docker0 $TAP
    # bring up tap devices
    sudo ip link set $TAP up
done
