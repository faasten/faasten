mkfile_path := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

all: nc-vsock outl do-snapshot ioctl factorial

nc-vsock: nc-vsock.c
	echo "apk add alpine-sdk linux-headers; gcc -o /common/nc-vsock /common/nc-vsock.c" | docker run -i --rm -v $(mkfile_path):/common alpine:3.10

outl: outl.c
	echo "apk add alpine-sdk linux-headers; gcc -o /common/outl /common/outl.c" | docker run -i --rm -v $(mkfile_path):/common alpine:3.10

do-snapshot: do-snapshot.c
	echo "apk add alpine-sdk linux-headers; gcc -o /common/do-snapshot /common/do-snapshot.c" | docker run -i --rm -v $(mkfile_path):/common alpine:3.10

ioctl: ioctl.c
	echo "apk add alpine-sdk linux-headers; gcc -o /common/ioctl /common/ioctl.c" | docker run -i --rm -v $(mkfile_path):/common alpine:3.10

factorial: factorial.c
	echo "apk add alpine-sdk linux-headers; gcc -o /common/factorial /common/factorial.c" | docker run -i --rm -v $(mkfile_path):/common alpine:3.10

.PHONY: clean
clean:
	rm -f nc-vsock outl do-snapshot ioctl factorial

