mkfile_path := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

all: output.ext2

__pycache__: workload.py
	echo "apk add python3; PYTHONPATH=/app python3 -m workload" | docker run -i --rm -v $(mkfile_path):/app -w /app alpine:3.10

output.ext2: __pycache__
	rm -rf output.ext2 /tmp/lorem.out/
	mkdir -p /tmp/lorem.out/
	touch output.ext2
	truncate -s 10M output.ext2
	mkfs.ext2 output.ext2
	sudo mount output.ext2 /tmp/lorem.out/
	sudo cp -r workload.py __pycache__ /tmp/lorem.out/
	sudo umount /tmp/lorem.out/

clean:
	sudo rm -rf __pycache__
	rm output.ext2
