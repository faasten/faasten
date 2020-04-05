mkfile_path := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

all: output.ext4

package: workload requirements.txt
	mkdir package
	echo "apk add python2 python2-dev py2-pip; pip2 install --target ./package -r requirements.txt" | docker run -i --rm -v $(mkfile_path):/app -w /app alpine

output.ext4: package
	rm -rf output.ext4 /tmp/lorem.out/
	mkdir -p /tmp/lorem.out/
	touch output.ext4
	truncate -s 10M output.ext4
	mkfs.ext4 output.ext4
	sudo mount output.ext4 /tmp/lorem.out/
	sudo cp -r package workload /tmp/lorem.out/
	sudo umount /tmp/lorem.out/
