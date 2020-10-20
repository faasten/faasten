## `default_env`
`default_env` defines default environment variables required by `setup.sh` and `run.sh` and scripts
in `setup_scripts` and `run_scripts`.

### Environment variable `RUNTIMES`
One can change the environment variable `RUNTIMES` to include or exclude language runtimes. By
default, `RUNTIMES=(python3 nodejs)`.


## `setup.sh` and `setup_scripts`
`setup.sh` run scripts located in `setup_scripts`. The flow of `setup.sh` is:
1. checks /ssd exists and an SSD device is mounted to it
2. makes /ssd writable to the current user
2. checks docker can run in non-root mode
4. `source default_env`
5. creates all required directories, namely `/tmp/snapfaas/release`, `/tmp/snapfaas/images/appfs`, `/tmp/snapfaas/images/rootfs`, `/tmp/snapfaas/snapshots`, `/tmp/snapfaas/snapshots/diff`, `/ssd/images/appfs`, `/ssd/images/rootfs`, `/ssd/snapashots`, `/ssd/snapshots/diff`, and `/tmp/snapfaas` is mounted a 20GB tmpfs.
6. copies `../resources/images/vmlinux-4.20.0` into `/tmp/snapfaas/images`
7. builds docker image smartdevice for nodejs alexa-door function
8. configures `tap0`
9. builds binaries, root filesystems, and app filesystems
10. if step 9 succeeds, builds diff snapshots, fullapp snapshots, and debug diff snapshots

### Control which script in `setup_scripts` will be run by `setup.sh`
`setup.sh` writes a unique word to `.stat` when a script in `setup_scripts` completes with success.
If such a word exists in `.stat`, running `setup.sh` will not run the corresponding script.

__NOTE__: All scripts in `setup_scripts` can be invoked directly. But users should make sure all prerequsites
are satisfied.

## `run.sh` and `run_scripts`
`run.sh` will run scripts located in `run_scripts`. All scripts in `run_scripts` checks whether all images/snapshots
are built and placed in the correct path by reading `.stat` and checking if the word 'setup' exists.
