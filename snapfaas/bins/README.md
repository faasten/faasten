# List of Binaries 
1. multivm: a FaaS backend server that runs multiple functions and receives requests from TCP connections.
2. singlevm: a tool that runs a single function and receives line-delimited JSON requests from the stdin. 
3. firerunner: a customized virtual machine manager based on firecracker that `multivm` and `singlevm` fork and run in a child process.
4. sfdb: a tool that injects key-value pairs into the specified lmdb database.
5. sfclient: a tool that sends requests over a TCP connection to `multivm`.
6. sffs: a tool that interacts with the labeled file system atop a lmdb database.
7. dropboxui: a tool that implements a dropbox-like UI for interacting with faasten's labeled file system.
