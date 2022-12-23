#!/usr/bin/env sh

/usr/bin/setup-eth0.sh
/usr/bin/ioctl

/bin/mount -r /dev/vdb /srv

# FIXME logfile cannot be created
# npm_config_cache=/tmp/npm npm --version
# npm config set cache /tmp/npm/ --loglevel=verbose
NODE_PATH=$NODE_PATH:$(npm root --quiet -g) node /bin/runtime-workload.js
