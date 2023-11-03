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
# System Administration Functions
* fsutil
  * This function is installed during the system bootstrapping as a gate at "home:<T,faasten>:fsutil"
    * The gate grants no privilege and allows anyone to invoke
* jwt
  * This function is installed during the system bootstrapping as a blob at "home:<faasten,faasten>:jwt"
  * This function is installed for each identity provider (idp) as a gate at "home:<{idp}|faasten,faasten>:jwt"
    * The gate grants "faasten" privilege so that an instance can read Faasten's private key
    * The gate allows "{idp}" to invoke, which implies an instance will run with the privilege "faasten&{idp}"
  * This function takes in a dict with "sub" key (standing for subject), generates a JWT for "sub", and registers for the authenticated user a private fsutil gate that redirects to the public fsutil gate mentioned above
    * The private gate will be at "home:<{idp}/{sub},{idp}/{sub}>:fsutil"
# TODO
- [] support for installing native library
- [] support for languages other than Python
- [] administration
  - [] install jwt during the system bootstrapping
  - [] admin tool for install a new idp
  - [] jwt registers fsutil
