FROM faasten:base

RUN apk add bash python3 python3-dev py3-pip py3-setuptools py3-protobuf py3-opencv py3-scikit-learn py3-pandas git cmake

ENV PYTHON python3.10

ADD workload.sh /bin/workload
RUN chmod +x /bin/workload

ADD workload.py /bin/runtime-workload.py

COPY faasten-0.1-py3-none-any.whl /tmp/
RUN pip3 install /tmp/faasten-0.1-py3-none-any.whl
RUN rm /tmp/faasten-0.1-py3-none-any.whl
