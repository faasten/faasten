#!/usr/bin/env sh

/usr/bin/setup-eth0.sh
/usr/bin/ioctl
# factorial $((1<<28))
# NODE_PATH=$NODE_PATH:/usr/lib/node_modules node /bin/runtime-workload.js
NODE_PATH=$NODE_PATH:$(npm root --quiet -g) node /bin/runtime-workload.js
