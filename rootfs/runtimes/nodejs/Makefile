PROTOSOURCE=$(abspath ../../../snapfaas/src/syscalls.proto)
all: syscalls_pb.js

syscalls_pb.js: $(PROTOSOURCE)
	@protoc --proto_path=$(dir $(PROTOSOURCE)) --js_out=import_style=commonjs,binary:. $(notdir $(PROTOSOURCE))

.PHONY: clean
clean:
	rm -f syscalls_pb.js
