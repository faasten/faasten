PROTOSOURCE=$(abspath ../../../snapfaas/src/syscalls.proto)
all: syscalls_pb2.py

syscalls_pb2.py: $(PROTOSOURCE)
	@protoc --proto_path=$(abspath ../../../snapfaas/src) --python_out=. syscalls.proto

.PHONY: clean
clean:
	rm -f syscalls_pb2.py
	rm -Rf google
