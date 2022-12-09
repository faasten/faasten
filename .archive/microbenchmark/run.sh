#!/usr/bin/env bash

if [ $# -ne 2 ]; then
    echo 'usage: ./run.sh START_INDEX NUMBER_OF_ROUNDS'
    exit 1
fi

# --load_dir base_dir,diff_dir
run_scripts/run_snapfaas.sh eager ssd $1 $2
# --load_dir base_dir,diff_dir --load_ws
run_scripts/run_snapfaas_reap.sh ondemand ssd $1 $2
# --load_dir base_dir
run_scripts/run_seuss.sh ondemand ssd $1 $2
# --laad_dir full_dir --load_ws
run_scripts/run_reap.sh ondemand ssd $1 $2
