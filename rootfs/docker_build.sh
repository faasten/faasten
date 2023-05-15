#!/bin/sh

docker build -q -t faasten:base .

CID=$(docker build -q runtimes/$1)
cidfile=$(mktemp --dry-run)
docker run --cidfile="$cidfile" "$CID" #sh -c 'ls'
tmpdir=$(mktemp -d)
docker export `cat $cidfile` | tar -xC "$tmpdir"

rm -f "$1.img"
gensquashfs --pack-dir "$tmpdir" "$1.img"
rm -Rf "$cidfile" "$tmpdir"
