#!/usr/bin/env bash
echo 'Building fc_wrapper and firerunner...'
cargo build --release --quiet --target-dir $MEMBINDIR --bins
