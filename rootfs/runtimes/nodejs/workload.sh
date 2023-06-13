#!/usr/bin/env sh

#/usr/bin/ioctl
#/usr/bin/setup-eth0.sh

/bin/mount -r /dev/vdb /srv

# FIXME logfile cannot be created
# npm_config_cache=/tmp/npm npm --version
# npm config set cache /tmp/npm/ --loglevel=verbose
NODE_PATH=$NODE_PATH:/usr/local/lib/node_modules node /bin/runtime-workload.js
