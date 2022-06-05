#!/usr/bin/env bash

#/usr/bin/setup-eth0.sh
#/usr/bin/ioctl
mkdir -p /tmp
/bin/mount -t tmpfs -o size=512m tmpfs /tmp
/bin/mount -r /dev/vdb /srv
LD_LIBRARY_PATH=/srv/lib PYTHONPATH=/srv:/srv/package python3 /bin/runtime-workload.py
