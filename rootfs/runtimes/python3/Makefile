PROTOSOURCE=$(abspath ../../../snapfaas/src/syscalls.proto)
all: faasten-0.1-py3-none-any.whl

.PHONY: faasten-0.1-py3-none-any.whl
faasten-0.1-py3-none-any.whl: faasten/faasten/syscalls_pb2.py
	python3 -m pip wheel -e faasten

faasten/faasten/syscalls_pb2.py: $(PROTOSOURCE)
	@protoc --proto_path=$(abspath ../../../snapfaas/src) --python_out=faasten/faasten syscalls.proto

.PHONY: clean
clean:
	rm -f syscalls_pb2.py
	rm -Rf google
