#!/bin/bash
# The first step on the host is to create a `tap` device:
sudo ip tuntap add tap0 mode tap
sudo ip tuntap add tap1 mode tap
sudo ip addr add 172.16.0.1/24 dev tap0
sudo ip addr add 172.16.0.1/24 dev tap1
# connect tap devices to `docker0` bridge
sudo brctl addif docker0 tap0
sudo brctl addif docker0 tap1
# bring up tap devices
sudo ip link set tap0 up
sudo ip link set tap1 up
