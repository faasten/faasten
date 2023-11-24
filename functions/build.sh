#!/bin/sh

set -e

rm -f output/$(basename $1).img
if [ -f $1/requirements.txt ]; then
	./docker_install.sh $(realpath $1)
fi
gensquashfs --pack-dir "$1" output/$(basename "$1").img
if [ -f $1/requirements.txt ]; then
	./docker_clean.sh $(realpath $1)
fi
