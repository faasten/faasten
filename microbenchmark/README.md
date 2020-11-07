## `default_env`
`default_env` defines default environment variables required by `setup.sh` and `run.sh` and scripts
in `setup_scripts` and `run_scripts`.

### Environment variable `RUNTIMES`
One can change the environment variable `RUNTIMES` to include or exclude language runtimes. By
default, `RUNTIMES=(python3 nodejs)`.


## `setup.sh` and `setup_scripts`
`setup.sh` run scripts located in `setup_scripts`. The flow of `setup.sh` is:
1. checks docker can run in non-root mode
2. copies `../resources/images/vmlinux-4.20.0` into `$MEMROOT/kernel`
3. builds docker image smartdevice for nodejs alexa-door function
4. configures `tap0` if not already configured
5. builds binaries, root filesystems, and app filesystems into `$MEMROOT/release`, `$MEMROOT/rootfs`, and `$MEMROOT/appfs`
6. if step 5 succeeds, builds diff snapshots and fullapp snapshots into `$MEMROOT/snapshots`

## `copyfs.sh`
copyfs.sh should be used to copy all filesystems and snapshots placed in $MEMROOT by `setup.sh` to
`$NVMROOT`, `$SSDROOT`, or `$HDDROOT`

### Control which script in `setup_scripts` will be run by `setup.sh`
`setup.sh` writes a unique word to `.stat` when a script in `setup_scripts` completes with success.
If such a word exists in `.stat`, running `setup.sh` will not run the corresponding script.

__NOTE__: All scripts in `setup_scripts` can be invoked directly. But users should make sure all prerequsites
are satisfied.

## `run.sh` and `run_scripts`
`run.sh` will run scripts located in `run_scripts`. All scripts in `run_scripts` checks whether all images/snapshots
are built and placed in the correct path by reading `.stat` and checking if the word 'setup' exists.
