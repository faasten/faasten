FROM faasten:base

RUN apk add bash python3 python3-dev py3-setuptools py3-protobuf py3-opencv py3-scikit-learn py3-pandas git cmake

ENV PYTHON python3.10

ADD workload.sh /bin/workload
RUN chmod +x /bin/workload

ADD workload.py /bin/runtime-workload.py

ADD syscalls.py /usr/lib/$PYTHON/syscalls.py
ADD syscalls_pb2.py /usr/lib/$PYTHON/syscalls_pb2.py

RUN echo hello $PYTHON
