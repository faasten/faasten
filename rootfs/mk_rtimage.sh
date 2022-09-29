#!/usr/bin/env bash

## Creates an alpine-linux based rootfs for a particular runtime.  ## All
## runtimes share a common prelude and postscript for initialization and are
## specialized by scripts defined in the `runtimes/` subdirectory (typically
## just an `apk` command to install the relevant runtime binaries and libraries).
##
## Usage
## -----
##
## $ ./mk_rtimage.sh [--local-snapfaas PATH] [--force_protoc] RUNTIME OUTPUT_FILE
##
## Where RUNTIME is one of the runtimes defined in `runtimes`, without the `.sh`
## extension, and OUTPUT is the file with the resulting root file system.
##
## The optional --local-snapfaas PATH indicates Makefiles of the runtime to use
## the given snapfaas repository instead of downloading from GitHub. The snapfaas
## respository contains the syscalls.proto file that `protoc` uses to generate the
## protobuf definitions for the target runtime. See runtimes/python3/Makefile for
## example.
##
## Running this script requires super user privileges to mount the target file
## system, but you don't have to run with `sudo`, the script uses `sudo` explicitly.
ALPINE=3.16
REV=0

function print_runtimes() {
  echo -e "Available runtimes:"
  for runtime_file in $(ls runtimes/)
  do
    echo -e "  * $(basename $runtime_file .sh)"
  done
}

function print_options() {
  echo
  echo "Options:"
  echo -e "    --local-snapfaas PATH\tpath of the local snapfaas repository's root"
  echo -e "    --force-protoc\tforce building protobuf definitions for the targer runtime"
}

PARAMS=""
while (( "$#" )); do
  case "$1" in
    --local-snapfaas)
      shift
      LOCALPATH="$1"
      shift
      ;;
    --force-protoc)
      FORCE='-B'
      shift
      ;;
    -*|--*=) # unsupported flags
      echo "Error: Unsupported flag $1" >&2
      print_options
      exit 1
      ;;
    *) # preserve positional arguments
      PARAMS="$PARAMS $1"
      shift
      ;;
  esac
done

eval set -- "$PARAMS"

## Check command line argument length
if [ "$#" -ne 2 ]; then
  echo "Usage: $0 [options] RUNTIME OUTPUT_FS_IMAGE"
  print_runtimes
  print_options
  exit 1
fi

RUNTIME=runtimes/$1
OUTPUT=$2

if [ ! -f "$RUNTIME"/rootfs.sh ]; then
  echo "Runtime \`$1\` not found."
  print_runtimes
  exit 1
fi

RUNTIME=$(realpath $RUNTIME)
MYDIR=$(dirname $(realpath $0))

make LOCALPATH=$LOCALPATH $FORCE -C $RUNTIME
make -C $MYDIR/../common

## Create a temporary directory to mount the filesystem
TMPDIR=`mktemp -d`

## Delete the output file if it exists, and create a new one formatted as
## an EXT4 filesystem.
rm -f $OUTPUT
truncate -s 500M $OUTPUT
mkfs.ext4 $OUTPUT

sudo mount $OUTPUT $TMPDIR

## Execute the prelude, runtime script and postscript inside an Alpine docker container
## with the target root file system shared at `/my-rootfs` inside the container.
cat $MYDIR/../common/prelude.sh $RUNTIME/rootfs.sh $MYDIR/../common/postscript.sh | docker run -i --rm -v $TMPDIR:/my-rootfs -v $MYDIR/../common:/common -v $RUNTIME:/runtime alpine:$ALPINE

# resize & cleanup
sudo umount $OUTPUT
e2fsck -f $OUTPUT
resize2fs -M $OUTPUT
rm -Rf $TMPDIR
