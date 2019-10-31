sudo groupadd -f snapfaas
sudo usermod -a -G snapfaas $USER
sudo mkdir -p /etc/snapfaas
sudo chown :snapfaas /etc/snapfaas
sudo chmod 775 /etc/snapfaas

cp ./bins/snapctr/default-conf.yaml /etc/snapfaas
cp ./vmlinux /etc/snapfaas

