make -C "runtimes/python3"

WORKDIR=$(pwd)
cd runtimes/python3
docker build --no-cache -t faasten:python3action -f Dockerfile-openwhisk-python3action .
cidfile=$(mktemp --dry-run)
docker run --cidfile="$cidfile" faasten:python3action # sh -c ls
tmpdir=$(mktemp -d)
docker export $(cat $cidfile) | tar -xC "$tmpdir"
cd "$WORKDIR"

rm -f "python3action.img"
gensquashfs --pack-dir "$tmpdir" "python3action.img"
rm -Rf "$cidfile" "$tmpdir"
