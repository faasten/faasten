SNAPFAAS=https://api.github.com/repos/princeton-sns/snapfaas/tarball/faasten
all: syscalls_pb2.py

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
	rm syscalls_pb2.py
