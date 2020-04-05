mkfile_path := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

all: nc-vsock outl

nc-vsock: nc-vsock.c
	echo "apk add alpine-sdk linux-headers; gcc -o /common/nc-vsock /common/nc-vsock.c" | docker run -i --rm -v $(mkfile_path):/common alpine

outl: outl.c
	echo "apk add alpine-sdk linux-headers; gcc -o /common/outl /common/outl.c" | docker run -i --rm -v $(mkfile_path):/common alpine
