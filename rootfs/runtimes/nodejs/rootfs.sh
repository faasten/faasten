apk add bash nodejs npm python3 make g++ linux-headers

npm install -g --unsafe-perm $(npm pack /runtime/vsock | tail -1)
npm install -g google-protobuf
# npm list -g

apk del python3 make g++ linux-headers

cp /runtime/workload.js /bin/runtime-workload.js
cp /runtime/syscalls_pb.js /bin/syscalls_pb.js
cp /runtime/syscalls.js /bin/syscalls.js
