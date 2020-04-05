sudo groupadd -f snapfaas
sudo usermod -a -G snapfaas $USER
sudo mkdir -p /etc/snapfaas
sudo chown :snapfaas /etc/snapfaas
sudo chmod 775 /etc/snapfaas

sudo cp ./resources/default-conf.yaml /etc/snapfaas
sudo cp ./resources/vmlinux /etc/snapfaas
sudo cp ./resources/example_function_configs.yaml /etc/snapfaas

# initialize submodules (for Firecracker)
echo "clone Firecracker repo..."
git submodule init
git submodule update

# build snapfaas binaries
echo "build snapfaas binaries..."
cargo build --release 2>/dev/null
cp ./target/release/firerunner /etc/snapfaas

# grant the current user permission to /dev/kvm
echo "grant current user ${USER} access to /dev/kvm"
sudo setfacl -m u:${USER}:rw /dev/kvm

# install necessary packages for data processing
pip3 install --upgrade pip
pip3 install pyyaml numpy matplotlib

# create /out directory
mkdir out

# configure permission to docker
sudo groupadd docker
sudo usermod -aG docker $USER
sudo service docker start

