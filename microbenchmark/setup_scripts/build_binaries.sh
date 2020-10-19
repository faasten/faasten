#!/usr/bin/env bash

# only source the environment file when invoked directly from command line
if [ $(ps -o stat= -p $PPID) == 'Ss' ]; then
	source ./default_env
fi
echo 'Building fc_wrapper and firerunner...'
cargo build --release --quiet --target-dir $MOUNTPOINT --bins
