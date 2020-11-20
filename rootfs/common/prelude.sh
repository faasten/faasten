apk add openrc util-linux

cp /common/outl /usr/bin/outl
cp /common/do-snapshot /usr/bin/do-snapshot
cp /common/setup-eth0.sh /usr/bin/setup-eth0.sh
cp /common/ioctl /usr/bin/ioctl

## Add /dev and /proc file systems to openrc's boot
rc-update add devfs boot
rc-update add procfs boot

## Create start script for that mounts the appfs and invokes whatever binary is in /srv/workload
cat /runtime/workload.sh > /bin/workload
chmod +x /bin/workload

## Have the start script invoked by openrc/init
printf '#!/sbin/openrc-run\n
command="/bin/workload"\n' > /etc/init.d/serverless-workload
chmod +x /etc/init.d/serverless-workload
rc-update add serverless-workload default

## specify default system library path
echo '/srv/lib:/lib:/usr/local/lib:/usr/lib' > /etc/ld-musl-$(uname -m).path
