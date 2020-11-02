#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: ./run.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi

run_scripts/run_diff.sh eager hdd $1 $2
run_scripts/run_diff.sh ondemand hdd $1 $2
run_scripts/run_fullapp.sh eager hdd $1 $2
run_scripts/run_fullapp.sh ondemand hdd $1 $2
run_scripts/run_regular.sh hdd $1 $2
