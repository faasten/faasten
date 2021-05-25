#!/usr/bin/env bash

setup_scripts/pre_build.sh

# prerequisites for building snapshots
# build root filesystems
setup_scripts/build_rootfs.sh
if [ $? -ne 0 ]; then
        tput setaf 1; echo 'Building root filesystems failed'
        tput sgr0
        exit 1
fi
# build app filesystems
setup_scripts/build_appfs.sh
if [ $? -ne 0 ]; then
        tput setaf 1; echo 'Building app filesystems failed'
        tput sgr0
        exit 1
fi
# build firerunner/fc_wrapper binaries
setup_scripts/build_binaries.sh
if [ $? -ne 0 ]; then
        tput setaf 1; echo 'Building binaries failed'
        tput sgr0
        exit 1
fi
# build language base snapshots + app diff snapshots
setup_scripts/build_diff_snapshots.sh
if [ $? -ne 0 ]; then
    tput setaf 1; echo 'Building base+diff snapshots failed'
    tput sgr0
    complete=0
fi
# build full-app snapshots
setup_scripts/build_fullapp_snapshots.sh
if [ $? -ne 0 ]; then
    tput setaf 1; echo 'Building fullapp snapshots failed'
    tput sgr0
    complete=0
fi
