ifconfig lo up
ip link set dev eth0 up
ip addr add 172.17.0.3/16 dev eth0
ip route add default via 172.17.0.1 dev eth0
