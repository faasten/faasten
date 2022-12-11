#!/bin/sh
if [ $# -ne 1 ]; then
    echo 'usage: ./cleanup-taps.sh NUMBER_OF_TAPS'
    exit 1
fi

MAXTAPID=$(($1-1))
for ((i=0; i<=$MAXTAPID; i++))
do
    sudo ip link delete tap$i
done
