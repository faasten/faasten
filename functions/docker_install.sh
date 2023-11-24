#!/bin/sh

set -e

docker build -q -t faasten:pythonfunc -f Dockerfile-python .
mkdir -p $1/package
echo $1
docker run --rm -v $1:/workdir -w /workdir faasten:pythonfunc pip3 install -r requirements.txt --target package
