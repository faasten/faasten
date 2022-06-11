import syscalls_pb2
import socket
import struct
import json

### helper functions ###
def recvall(sock, n):
    # Helper function to recv n bytes or return None if EOF is hit
    data = bytearray()
    while len(data) < n:
        packet = sock.recv(n - len(data))
        if not packet:
            return None
        data.extend(packet)
    return data

def protorelpath(value):
    ret = syscalls_pb2.Path()
    ret.value = value
    ret.pathT = syscalls_pb2.PathType.REL_WORKSPACE
    return ret

def protoabspath(value):
    ret = syscalls_pb2.Path()
    ret.value = value
    ret.pathT = syscalls_pb2.PathType.ABS
    return ret
### end of helper functions ###

class Syscall():
    def __init__(self, sock):
        self.sock = sock

    def _send(self, obj):
        objData = obj.SerializeToString()
        try:
            self.sock.sendall(struct.pack(">I", len(objData)))
            self.sock.sendall(objData)
        except:
            while True:
                continue

    def _recv(self, obj):
        data = self.sock.recv(4, socket.MSG_WAITALL)
        res = struct.unpack(">I", data)
        objData = recvall(self.sock, res[0])

        obj.ParseFromString(objData)
        return obj

    def request(self):
        request = syscalls_pb2.Request()
        return self._recv(request)

    def respond(self, response):
        response = syscalls_pb2.Syscall(response = syscalls_pb2.Response(payload = json.dumps(response)))
        self._send(response)

    def write_key(self, key, value):
        req = syscalls_pb2.Syscall(writeKey = syscalls_pb2.WriteKey(key = key, value = value))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def read_key(self, key):
        req = syscalls_pb2.Syscall(readKey = syscalls_pb2.ReadKey(key = key))
        self._send(req)
        response = self._recv(syscalls_pb2.ReadKeyResponse())
        return response.value

    ### label APIs ###
    def get_current_label(self):
        req = syscalls_pb2.Syscall(getCurrentLabel = syscalls_pb2.GetCurrentLabel())
        self._send(req)
        response = self._recv(syscalls_pb2.DcLabel())
        return response

    def taint(self, label):
        req = syscalls_pb2.Syscall(taintWithLabel = label)
        self._send(req)
        response = self._recv(syscalls_pb2.DcLabel())
        return response

    def declassify(self, secrecy: syscalls_pb2.DcComponent):
        """Declassify to the target secrecy and leave integrity untouched.
        """
        req = syscalls_pb2.Syscall(declassify = syscalls_pb2.DcComponent(value=secrecy))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success
    ### end of label APIs ###

    ### github APIs ###
    def github_rest_get(self, route):
        req = syscalls_pb2.Syscall(githubRest = syscalls_pb2.GithubRest(verb = syscalls_pb2.HttpVerb.GET, route = route, body = None))
        self._send(req)
        response= self._recv(syscalls_pb2.GithubRestResponse())
        return response

    def github_rest_post(self, route, body):
        bodyJson = json.dumps(body)
        req = syscalls_pb2.Syscall(githubRest = syscalls_pb2.GithubRest(verb = syscalls_pb2.HttpVerb.POST, route = route, body = bodyJson))
        self._send(req)
        response= self._recv(syscalls_pb2.GithubRestResponse())
        return response

    def github_rest_put(self, route, body):
        bodyJson = json.dumps(body)
        req = syscalls_pb2.Syscall(githubRest = syscalls_pb2.GithubRest(verb = syscalls_pb2.HttpVerb.PUT, route = route, body = bodyJson))
        self._send(req)
        response= self._recv(syscalls_pb2.GithubRestResponse())
        return response

    def github_rest_delete(self, route, body):
        bodyJson = json.dumps(body)
        req = syscalls_pb2.Syscall(githubRest = syscalls_pb2.GithubRest(verb = syscalls_pb2.HttpVerb.DELETE, route = route, body = bodyJson))
        self._send(req)
        response= self._recv(syscalls_pb2.GithubRestResponse())
        return response
    ### end of github APIs ###

    def invoke(self, function, payload):
        req = syscalls_pb2.Syscall(invoke = syscalls_pb2.Invoke(function = function, payload = payload))
        self._send(req)
        response= self._recv(syscalls_pb2.InvokeResponse())
        return response.success

    ### file system APIs ###
    def fs_read(self, path):
        """Read the file at path `path`."""
        req = syscalls_pb2.Syscall(fsRead = syscalls_pb2.FSRead(path = protoabspath(path)))
        self._send(req)
        response = self._recv(syscalls_pb2.ReadKeyResponse())
        return response.value

    def fs_write(self, path, data):
        """Overwrite the file at path `path` with data `data`.
        The backend handler always endorse before writing.
        """
        req = syscalls_pb2.Syscall(fsWrite = syscalls_pb2.FSWrite(path = protoabspath(path), data = data))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createdir(self, name, path, label: syscalls_pb2.DcLabel=None):
        """Create a directory `name` with label `label` in the path `path`.
        The backend handler always endorse before creating the directory.
        """
        req = syscalls_pb2.Syscall(fsCreateDir = syscalls_pb2.FSCreate(
            basePath=protoabspath(path), name=name, label=label,
            entryT=syscalls_pb2.DentryType.DIR))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createfile(self, name, path, label: syscalls_pb2.DcLabel=None):
        """Create a file `name` with label `label` in the path `path`.
        The backend handler always endorse before creating the file.
        """
        req = syscalls_pb2.Syscall(fsCreateFile = syscalls_pb2.FSCreate(
            basePath=protoabspath(path), name=name, label=label,
            entryT=syscalls_pb2.DentryType.FILE))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def workspace_createdir(self, name, path='.', label: syscalls_pb2.DcLabel=None):
        """Create a directory `name` labeled `label` in $workspace/`path`.

        The current workspace directory $workspace is /$functionName/$endUser.
        The backend handler is responsible for substituting the correct values.
        The backend handler always endorses with the function's full privilege
        before creating the directory.

        Args:
            name:
                The name of the new directory.
            path:
                The path of the new directory's base directory, relative to the workspace directory.
                The default value is '.', the workspace directory itself.
            label:
                The new directory's label. The default value is None, indicating
                the backend handler to use the function's label after endorsement.

        Returns:
            A bool that indicates whether the creation succeeds or not.
        """
        req = syscalls_pb2.Syscall(fsCreate=syscalls_pb2.FSCreate(
            basePath=protorelpath(path), name=name, label=label,
            entryT=syscalls_pb2.DentryType.DIR))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def workspace_createfile(self, name, path='.', label: syscalls_pb2.DcLabel=None):
        """See workspace_createdir."""
        req = syscalls_pb2.Syscall(fsCreate=syscalls_pb2.FSCreate(
            basePath=protorelpath(path), name=name, label=label,
            entryT=syscalls_pb2.DentryType.FILE))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def workspace_write(self, path, data):
        """ Overwrite the file at $workspace/`path` with `data`.
        
        More details, see workspace_createdir.

        Args:
            path:
                The path of the file, relative to the workspace directory.
            data:
                The data to overwrite with.

        Returns:
            A bool that indicates whether the write succeeds or not.
        """
        req = syscalls_pb2.Syscall(fsWrite=syscalls_pb2.FSWrite(path=protorelpath(path), data=data))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def workspace_read(self, path):
        """Read the file at $workspace/`path`.
        
        More details about $workspace, see workspace_createdir.

        Args:
            path:
                The path of the file, relative to the workspace directory.

        Returns:
            A [u8] holding the data if the read succeeds. None, otherwise.
        """
        req = syscalls_pb2.Syscall(fsWrite=syscalls_pb2.FSWrite(path=protorelpath(path), data=data))
        self._send(req)
        response = self._recv(syscalls_pb2.ReadKeyResponse())
        return response.value

    def workspace_abspath(self, relpath):
        """Return the absolute path of the `relpath`."""
        req = syscalls_pb2.Syscall(workspaceAbspath=syscalls_pb2.WorkspaceAbspath(relpath=relpath))
        self._send(req)
        response = self._recv(syscalls_pb2.WorkspaceAbspathResponse())
        return response.abspath
    ### end of file system APIs ###
