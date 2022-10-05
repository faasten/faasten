#!/usr/bin/env sh
NODE_PATH=$NODE_PATH:$(npm root --quiet -g) node /bin/runtime-workload.js
