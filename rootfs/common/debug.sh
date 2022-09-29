## Setup console
ln -s agetty /etc/init.d/agetty.ttyS0
echo ttyS0 > /etc/securetty
rc-update add agetty.ttyS0 default
rc-update add agetty.ttyS0 nonetwork

echo agetty_options=\"-a root\" >> /etc/conf.d/agetty

