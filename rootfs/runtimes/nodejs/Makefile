SNAPFAAS=https://api.github.com/repos/princeton-sns/snapfaas/tarball/master

all: syscalls_pb.js

syscalls_pb.js:
ifdef LOCALPATH
	@echo "Using local snapfaas at $(LOCALPATH)"
	@protoc --proto_path=$(LOCALPATH)/snapfaas/src --js_out=. syscalls.proto
else
	@echo 'Using snapfaas github repository'
	@curl -sL -H "Accept: application/vnd.github.v3+json" $(SNAPFAAS) | \
	tar xzf - --strip-components=3 --wildcards '*/snapfaas/src/syscalls.proto'
	@protoc --proto_path=. --js_out=import_style=commonjs,binary:. syscalls.proto
	@rm syscalls.proto
endif

.PHONY: clean
clean:
	rm -f syscalls_pb.js
