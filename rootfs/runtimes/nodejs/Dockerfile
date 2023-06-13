FROM faasten:base

RUN apk add bash nodejs npm python3 git cmake make g++ linux-headers

RUN npm install -g google-protobuf

COPY vsock-1.0.0.tgz /tmp/vsock-1.0.0.tgz
RUN npm install -g --unsafe-perm /tmp/vsock-1.0.0.tgz

ADD syscalls_pb.js /bin/syscalls_pb.js
ADD syscalls.js /bin/syscalls.js

ADD workload.sh /bin/workload
RUN chmod +x /bin/workload
ADD workload.js /bin/runtime-workload.js
