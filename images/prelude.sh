apk add openrc util-linux

cp /common/nc-vsock /usr/bin/nc-vsock
cp /common/outl /usr/bin/outl

## Create start script for that mounts the appfs and invokes whatever binary is in /srv/workload
printf '#!/bin/sh\n
stty -F /dev/ttyS1 -echo >/dev/null\n
exec /bin/runtime-workload\n' > /bin/workload
chmod +x /bin/workload

## Have the start script invoked by openrc/init
printf '#!/sbin/openrc-run\n
command="/bin/workload"\n' > /etc/init.d/serverless-workload
chmod +x /etc/init.d/serverless-workload
rc-update add serverless-workload default

## Add /dev and /proc file systems to openrc's boot
rc-update add devfs boot
rc-update add procfs boot
