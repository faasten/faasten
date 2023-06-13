FROM  docker.io/library/alpine:3.16

ADD common/setup-eth0.sh /usr/bin/setup-eth0.sh
ADD common/ioctl /usr/bin/ioctl

RUN apk add openrc util-linux build-base ca-certificates rng-tools rng-tools-openrc
RUN rc-update add devfs boot
RUN rc-update add procfs boot
RUN rc-update add rngd boot

RUN printf '#!/sbin/openrc-run\n \
    command="/bin/workload"\n' > /etc/init.d/serverless-workload
RUN chmod +x /etc/init.d/serverless-workload
RUN rc-update add serverless-workload default

RUN echo -n > /etc/fstab
