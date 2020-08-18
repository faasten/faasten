#!/usr/bin/env bash
source ./env
echo 'Building fc_wrapper and firerunner...'
cargo build --release --quiet --target-dir $MOUNTPOINT/bin --bins
