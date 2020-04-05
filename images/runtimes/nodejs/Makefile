mkfile_path := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

all: ioctl

ioctl: ioctl.c
	echo "apk add alpine-sdk linux-headers; gcc -o /runtime/ioctl /runtime/ioctl.c" | docker run -i --rm -v $(mkfile_path):/runtime alpine
