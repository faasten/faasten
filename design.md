# Front-ends
## Authenticator
### Input
### Output
### Job
The authenticator establishes the identity of a client. The identity is used as the security principal
throughout Faasten.
## Event server
### Input
### Output
### Job
The event server processes requests
# Core
## Label-oblivious scheduler
### Input
A LabeledInvoke RPC from the event server or a worker.
### Output
The input LabeledInvoke RPC to a worker.
### Side effect

### Job
## Label-aware workers
## Labeled global file system
