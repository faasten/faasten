#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: ./run.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi

source ./default_env

run_scripts/run_diff.sh $1 $2
run_scripts/run_fullapp.sh $1 $2
run_scripts/run_regular.sh $1 $2
