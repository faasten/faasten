apk add bash python3 python3-dev py3-setuptools py3-opencv py3-scikit-learn py3-pandas git cmake
PYTHON=$(ls /usr/lib | grep '\<python')

cp -r /runtime/google /usr/lib/$PYTHON/google
cp /runtime/workload.py /bin/runtime-workload.py
cp /runtime/syscalls.py /usr/lib/$PYTHON/syscalls.py
cp /runtime/syscalls_pb2.py /usr/lib/$PYTHON/syscalls_pb2.py
