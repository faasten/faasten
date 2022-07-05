SNAPFAAS=https://api.github.com/repos/princeton-sns/snapfaas/tarball/faasten
PROTOBUF_WHL_URL=https://files.pythonhosted.org/packages/9d/82/b3131637daf2a27eab76b0de8e139ecf0f6624832c03531dce8a7d59ddc1/protobuf-4.21.0-cp37-abi3-manylinux2014_x86_64.whl
all: google/protobuf syscalls_pb2.py


google/protobuf:
	wget -O protobuf.whl $(PROTOBUF_WHL_URL)
	unzip protobuf.whl
	rm protobuf.whl
	rm -Rf protobuf-*.dist-info

syscalls_pb2.py:
ifdef LOCALPATH
	@echo "Using local snapfaas at $(LOCALPATH)"
	@protoc --proto_path=$(LOCALPATH)/snapfaas/src --python_out=. syscalls.proto
else
	@echo 'Using snapfaas github repository'
	@curl -sL -H "Accept: application/vnd.github.v3+json" $(SNAPFAAS) | \
	tar xzf - --strip-components=3 --wildcards '*/snapfaas/src/syscalls.proto'
	@protoc --proto_path=. --python_out=. syscalls.proto
	@rm syscalls.proto
endif

.PHONY: clean
clean:
	rm -f syscalls_pb2.py
	rm -Rf google
