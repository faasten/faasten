#!/bin/bash
# Taken out from firecracker repo's `docs/network-setup.md`
# The first step on the host is to create a `tap` device:
sudo ip tuntap add tap0 mode tap
sudo ip tuntap add tap1 mode tap
#Then you have a few options for routing traffic out of the tap device, through
#your host's network interface. One option is NAT, set up like this:
sudo brctl addif docker0 tap0
sudo brctl addif docker0 tap1

sudo ip link set tap0 up
sudo ip link set tap1 up
