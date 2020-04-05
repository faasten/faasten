## Make sure fstab exists and is empty
echo -n > /etc/fstab

## Copy all relevant useful directories to /my-rootfs, where the target filesystem is mounted
for d in bin etc lib root sbin usr home srv; do tar c "$d" | tar x -C /my-rootfs; done

## Create empty directories for remaining folders
for dir in tmp dev proc run sys var; do mkdir /my-rootfs/${dir}; done

## Replace /sbin/init with our customized one
#rm /my-rootfs/sbin/init
#cp /common/init /my-rootfs/sbin

exit
