# Build Function Images
* pre-req: docker and gensquashfs (included in squashfs-tools-ng on Ubuntu)
* run:
```sh
# The script detects if there is a requirements.txt file.
# If the file exists, the script runs ./docker_install.sh
# to install the packages (e.g., jwt).
# The output is at ./output/*.img
./build.sh ./path/to/function/root
```
# TODO
* support for installing native library
* support for languages other than Python
