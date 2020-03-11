sudo groupadd -f snapfaas
sudo usermod -a -G snapfaas $USER
sudo mkdir -p /etc/snapfaas
sudo chown :snapfaas /etc/snapfaas
sudo chmod 775 /etc/snapfaas

sudo cp ./bins/snapctr/default-conf.yaml /etc/snapfaas
sudo cp ./resources/vmlinux /etc/snapfaas
sudo cp ./resources/example_function_configs.yaml /etc/snapfaas

# initialize submodules (for Firecracker)
git submodule init
git submodule update

# build snapfaas binaries
cargo build --release 2>/dev/null
cp ./target/release/firerunner /etc/snapfaas
