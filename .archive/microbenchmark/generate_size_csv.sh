#!/usr/bin/env bash

if [ $# -ne 1 ]; then
    echo 'usage: ./generate_size_csv.sh SNAPSHOT_ROOT_DIRECTORY'
    exit 1
fi

ROOT=$1

source ./default_env

# header 1 
echo -n 'base snapshot sizes (Bytes),'
for runtime in "${RUNTIMES[@]}"
do
    echo -n $runtime,
done
echo

# row
echo -n 'base snapshot sizes,'
for runtime in "${RUNTIMES[@]}"
do
    echo -n "$(ls -l $ROOT/$runtime/memory_dump | awk '{print $5}')",
done
echo

# header
echo -n 'sizes of different snapshots (Bytes),'
for snapshot in "${RUNAPPS[@]}"
do
    echo -n $snapshot,
done
echo

# rows
# row 1
echo -n 'diff-WS,'
for snapshot in "${RUNAPPS[@]}"
do
    echo -n "$(ls -l $ROOT/diff/$snapshot/WS_dump | awk '{print $5}')",
done
echo

# row 2
echo -n 'diff,'
for snapshot in "${RUNAPPS[@]}"
do
    echo -n "$(ls -l $ROOT/diff/$snapshot/memory_dump | awk '{print $5}')",
done
echo

# row 3
echo -n 'full-WS,'
for snapshot in "${RUNAPPS[@]}"
do
    echo -n "$(ls -l $ROOT/$snapshot/WS_dump | awk '{print $5}')",
done
echo

# row 4
echo -n 'full,'
for snapshot in "${RUNAPPS[@]}"
do
    echo -n "$(ls -l $ROOT/$snapshot/memory_dump | awk '{print $5}')",
done
echo

# row 5
echo -n 'diff-WS metadata,'
for snapshot in "${RUNAPPS[@]}"
do
    echo -n "$(ls -l $ROOT/diff/$snapshot/snapshot.json | awk '{print $5}')",
done
echo

# row 6
echo -n 'full-WS metadata,'
for snapshot in "${RUNAPPS[@]}"
do
    echo -n "$(ls -l $ROOT/$snapshot/snapshot.json | awk '{print $5}')",
done
echo
