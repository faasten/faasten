#!/bin/sh

set -e


rm -f `basename $1`.img
if [ -f $d/Makefile ]; then
  ./docker_build.sh "$1" output/$(basename "$1").img
else
  gensquashfs --pack-dir "$1" output/$(basename "$1").img
fi
