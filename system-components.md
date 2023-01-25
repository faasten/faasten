# Front-ends
## Authenticator
### Input
An HTTP request for authentication.
### Output
An HTTP response indicating success/failure.
### Job
The authenticator establishes the identity of a client. The identity is used as the security principal
throughout Faasten.

## API gateway (WIP)
### Input
An HTTP request from a authenticated client.
### Output
An HTTP response depending on the request.
### Job
The API gateway provides the user-facing web API endpoints that enable invocation and storage operation.
1. `POST /invoke`. Invoke the gate. The request body is a JSON string containing the keys:
```json
{
  "path": "path/to/the/gate",
  "payload": "JSON string",
  "label": serialized Rust DCLabel
}
```
The response indicates whether the invocation is submitted to the scheduler.

3. `POST /gates`. Create a new gate at the path specified in the payload. The created gate links
the blob specified in the payload.
```json
{
  "path": "path/to/the/gate",
  "blob": "BLOB"
}
```

4. `PUT /gates`. Update an existing gate at the path specified in the payload to link

4. `POST /dirs/dir/path/to/the/object`. Create a gate. The request body specifies at what path to
create the gate and the function image the gate links. The response indicates the success of the creation.

5. `PUT /directories/path/to/the/object`. Update the linked function image.

## Event server (now we have an ad-hoc solution for GitHub webhook events)
### Job
The event server support automated invocation. For example, a push to a GitHub
repository posts a webhook event to the server. The server generates a
payload from the event and submits to the scheduler.

# Core
## Label-oblivious scheduler
### Input
An invoke RPC from the API gateway, the event server or a worker.
### Output
The input invoke RPC to a worker.
### Job
The scheduler distibutes a queue of invocation requests to idle workers.
The invoke RPC message format (in protobuf):
```proto
message{
  Component invokerPrivilege, //the owned privilege of the invoker
  repeated PathComponent gate,
  string payload,
  Buckle payloadLabel,
}
```

## Label-aware workers
### Input
A LabeledInvoke struct
### Output
N/A
### Job
A worker is either idle waiting for invocation requests from the scheduler or
occupied processing an invocation to completion (acts as the invokee). A function runs in a VM.

#### Invocation authorization & function instance privilege
Functions are named by one or more gates in the global file system. A gate stores
a hard link to the underlying function image. Moreover, a gate stores the security
policies on the underlying function as well, including the policies who can invoke
and what privilege the instance will run with.
```txt
            | inst1 | inst2 | inst3 |
            |/|     |/|     |/|     |/|
 ------------>|------>|------>|------>|   ...
 information / flow  /       /       /
            |   |   |       |       |
            |   |   |       |       |
          gate1 | gate2   gate3   gate4
                |
               \|/
               ___
           ___|   |___ external resource gate (e.g. GitHub repo resource)

```
For an invocation, there is an invoker and an invokee. The invokee is always a worker but the
invoker can be an client (through the API gateway or the event server). The invokee worker is
responsible for checking the authorization and set the instance's privilege accordingly.

The invoker is identified by its owned privilege. The invoke RPC message includes the value
`invokePrivilege`. The invokee worker checks if `invokePrivilege` implies the invokee gate's invoking
policy.

The API gateway sets the `invokePrivilege` field to `[idstring]`, where `idstring` is the
authenticated client's identity string. The event server sets the field according
to an event's configuration. An invoker worker sets the field to the thread-local variable
`OWNED_PRIVILEGE`.

The invokee worker first traverses the file system to read the gate. Traversing the file system
implicitly raises the current computation's label.

A worker maintains a floating label for each invocation/computation. The label
starts as the public label. Then, it gets tainted with 1) the gate's label and those of its parents;
2) the payload's label; 3) the VM's label (see below).

If authorized, a worker either runs the invocation in a less tainted than the
payload, idle VM on the local machine or, if no such VM exists, it runs the
invocation in a new allocated, untainted VM. When an invocation completes,
a worker caches the VM with its floating label at the completion.

## Labeled global file system
### Job
All persistent states live in the global file system. These states include
directories, files, function gates, external gates, and trigger events.
