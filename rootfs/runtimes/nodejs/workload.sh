#!/usr/bin/env sh

/bin/mount -r /dev/vdb /srv

NODE_PATH=$NODE_PATH:/usr/local/lib/node_modules node /bin/runtime-workload.js
