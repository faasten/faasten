# snapctr

Start SnapFaaS by running snapctr--the SnapFaaS controller. It starts a gateway
for accepting requests and a pool of worker threads for handling requests. The
gateway either waits for requests on a port (HTTPGateway) or reads requests
from a file (FileGateway). Requests are represented as JSON strings. See
`resources/example_requests.json` for examples.

The gateway takes requests as input and output a `(Request, Channel)` tuple.
`Request` is the data structure that represents a function invocation request.
`Channel` is the response channel. This is a client TCP connetion with
`HTTPGateway` and a `Sender` to a single response serializer thread with
`FileGateway`.

`snapctr` then takes the `(Request, Channel)` tuple and forward it to the
worker pool. The worker pool consists of a pool of worker threads that are
initialized and launched before the gateway starts accepting requests.  Pool
size is `total memory/128`, where 128(MB) is the smallest VM size we support.

All threads in the worker pool share the receiver end of a MPSC channel
protected by a mutex (i.e., `Arc<Mutex<Receiver>>`) and the controller holds the
sender end. All worker threads try to grab the lock on the receiver and then
try to receive. Combining a MPSC channel with a mutex makes it simple to
guarantee that all worker threads hang waiting for requests (not busy waiting),
only one worker can receive at any given time and immediately after a worker
finishes receiving, another worker can start receiving. See `Worker::new()`
for more details.

Once a worker receives a `(Request, Channel)` tuple, it tries to acquire a VM
with the requested function loaded, forward it the request, wait for the VM to
finish processing the request and respond, and finally send the response back
to the client through the `Channel`.

SnapFaaS uses Firecracker VMs and run them in processes separate from the
controller. After processing a request, a VM does not shutdown. Instead it
stays idle waiting for another request. Note that each VM in SnapFaaS has only
a single function loaded. Therefore each VM can only handle a single type of
requests.

Worker threads (running inside the controller) are responsible for acquiring
VMs to handle requests. To acquire a VM, a worker first tries to find an idle
VM capable of handling the request. Such a VM needs to have the requested
function loaded and is currently not processing another request. If the worker
cannot find an suitable idle VM, it then tries to launch a VM with the right
function loaded. It does this by checking if there's enough memory to launch a
VM with the target function (different functions require VMs with different
amount of memory) and then spawning a `firerunner` process with the right input
parameters. If there's not enough memory left, the worker will try to kill idle
VMs of other functions to free spaces. It first checks whether there's enough
idle memory, if once freed, to launch its desired VM. If yes, it then kills
those VMs and then launch its VM. If no, it fails the request and returns an
error to client, indicating resource exhaustion.

See `snapctr --help` for more details.

# firerunner

`firerunner` is a lightweight wrapper for Firecracker VMM. `snapctr` uses it to
launch new VMs with specified functions loaded. We can also run `firerunner`
independently to launch standalone VMs that are not managed by `snapctr`.

See `firerunner --help` for more details.

