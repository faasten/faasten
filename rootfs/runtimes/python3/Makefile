PROTOBUF_WHL_URL=https://files.pythonhosted.org/packages/9d/82/b3131637daf2a27eab76b0de8e139ecf0f6624832c03531dce8a7d59ddc1/protobuf-4.21.0-cp37-abi3-manylinux2014_x86_64.whl
all: google/protobuf syscalls_pb2.py


google/protobuf:
	wget -O protobuf.whl $(PROTOBUF_WHL_URL)
	unzip protobuf.whl
	rm protobuf.whl
	rm -Rf protobuf-*.dist-info

syscalls_pb2.py:
	@protoc --proto_path=$(abspath ../../../snapfaas/src) --python_out=. syscalls.proto

.PHONY: clean
clean:
	rm -f syscalls_pb2.py
	rm -Rf google
