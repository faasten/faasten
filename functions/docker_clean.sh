#!/bin/sh

docker run --rm -v $1:/workdir -w /workdir faasten:pythonfunc rm -r package
