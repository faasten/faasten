source ./default_env
sudo umount $MEMROOT
rm -rf $SSDROOT/*
rm -rf $HDDROOT/*
rm -rf $NVMROOT/*
rm .stat
