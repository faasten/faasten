#!/usr/bin/env sh

/usr/bin/setup-eth0.sh
/usr/bin/ioctl

/bin/mount -r /dev/vdb /srv
NODE_PATH=$NODE_PATH:$(npm root --quiet -g) node /bin/runtime-workload.js
