#!/bin/sh

function print_runtimes() {
  echo -e "Available runtimes:"
  for runtime_file in $(ls runtimes/)
  do
    echo -e "  * $(basename $runtime_file .sh)"
  done
}

## Check command line argument length
if [ "$#" -ne 1 ]; then
  echo "Usage: $0 RUNTIME"
  print_runtimes
  exit 1
fi

docker build -q -t faasten:base .
make -C "runtimes/$1"

CID=$(docker build -q runtimes/$1)
cidfile=$(mktemp --dry-run)
docker run --cidfile="$cidfile" "$CID" #sh -c 'ls'
tmpdir=$(mktemp -d)
docker export `cat $cidfile` | tar -xC "$tmpdir"

rm -f "$1.img"
gensquashfs --pack-dir "$tmpdir" "$1.img"
rm -Rf "$cidfile" "$tmpdir"
