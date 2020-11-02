#!/usr/bin/env bash

source ./default_env

echo 'Building fc_wrapper and firerunner...'
cargo build --release --quiet --target-dir $MOUNTPOINT --bins
