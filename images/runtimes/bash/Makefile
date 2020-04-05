mkfile_path := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

all: workload

workload: workload.c
	echo "apk add alpine-sdk linux-headers; gcc -o /runtime/workload /runtime/workload.c" | docker run -i --rm -v $(mkfile_path):/runtime alpine
