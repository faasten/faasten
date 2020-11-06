#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: ./run.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi

run_scripts/run_diff.sh eager memmem $1 $2
run_scripts/run_diff.sh ondemand memmem $1 $2
run_scripts/run_fullapp.sh eager memmem $1 $2
run_scripts/run_fullapp.sh ondemand memmem $1 $2
run_scripts/run_regular.sh mem $1 $2
