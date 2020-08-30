#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: ./run_ocr_all.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi
./run_ocr_diff.sh $1 $2
./run_ocr_fullapp.sh $1 $2
./run_ocr_regular.sh $1 $2
