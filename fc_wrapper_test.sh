#echo "No snapshot"
#echo "loremjs"
#cat resources/loremjs.json | ./target/release/fc_wrapper --id 1 --kernel foo --mem_size 128 --rootfs /etc/snapfaas/resources/runtimefs/nodejs.ext4 --vcpu_count 2 --appfs /etc/snapfaas/resources/appfs/loremjs.ext4 
#echo ""
echo "Snapshot"
./target/release/fc_wrapper --id 1 --kernel /etc/snapfaas/vmlinux --mem_size 128 --rootfs /etc/snapfaas/resources/runtimefs/nodejs.ext4 --vcpu_count 1 --appfs /etc/snapfaas/resources/appfs/loremjs.ext4 --load_dir /etc/snapfaas/resources/snapshots/nodejs-128/
