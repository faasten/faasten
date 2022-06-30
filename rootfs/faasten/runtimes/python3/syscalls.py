import syscalls_pb2
import socket
import struct
import json
from contextlib import contextmanager

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

    ### named data object syscalls ###
    def fs_read(self, path):
        """Read the file at path `path`."""
        req = syscalls_pb2.Syscall(fsRead = syscalls_pb2.FSRead(path = path))
        self._send(req)
        response = self._recv(syscalls_pb2.ReadKeyResponse())
        return response.value

    def fs_write(self, path, data):
        """Overwrite the file at path `path` with data `data`.
        The backend handler always endorse before writing.
        """
        req = syscalls_pb2.Syscall(fsWrite = syscalls_pb2.FSWrite(path = path, data = data))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createdir(self, name, path, label: syscalls_pb2.DcLabel=None):
        """Create a directory `name` with label `label` in the path `path`.
        The backend handler always endorse before creating the directory.
        """
        req = syscalls_pb2.Syscall(fsCreateDir = syscalls_pb2.FSCreateDir(
            basePath = path, name = name, label = label))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createfile(self, name, path, label: syscalls_pb2.DcLabel=None):
        """Create a file `name` with label `label` in the directory `path`.
        The backend handler always endorse before creating the file.
        """
        req = syscalls_pb2.Syscall(fsCreateFile = syscalls_pb2.FSCreateFile(
            basePath = path, name = name, label = label))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success
    ### end of named data object syscalls ###

    ### unnamed data object syscalls #
    @contextmanager
    def create_unnamed(self, size: int = None):
        """Create a nameless data object.

        The implementation uses the content-addressed blob store.

        Yield:
            An instance of class NewBlob
        """
        req = syscalls_pb2.Syscall(createBlob=syscalls_pb2.BlobCreate(size=size))
        self._send(req)
        response = self._recv(syscalls_pb2.BlobResponse())
        if response.success:
            fd = response.fd
            yield NewBlob(fd, self)
            syscalls_pb2.Syscall(closeBlob=syscalls_pb2.BlobClose(fd=fd))
            self._send(req)
            response = self._recv(syscalls_pb2.BlobResponse())
        else:
            raise CreateUnnamedError

    @contextmanager
    def open_unnamed(self, name):
        """Open an existing nameless data object read-only.

        The implementation uses the content-addressed blob store.

        Yield:
            An instance of class Blob
        """
        req = syscalls_pb2.Syscall(openBlob=syscalls_pb2.BlobOpen(name=name))
        self._send(req)
        response = self._recv(syscalls_pb2.BlobResponse())
        fd = response.fd
        yield Blob(fd, self)
        req = syscalls_pb2.Syscall(closeBlob=syscalls_pb2.BlobClose(fd=fd))
        self._send(req)
        response = self._recv(syscalls_pb2.BlobResponse())
    ### end of unnamed object syscalls ###

class NewBlob():
    def __init__(self, fd, syscall):
        self.fd = fd
        self.syscall = syscall

    def write(self, data):
        req = syscalls_pb2.Syscall(writeBlob=syscalls_pb2.BlobWrite(fd=self.fd, data=data))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.BlobResponse())
        return response.success

    def finalize(self, data):
        req = syscalls_pb2.Syscall(finalizeBlob=syscalls_pb2.BlobFinalize(fd=self.fd, data=data))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.BlobResponse())
        return response.data.decode("utf-8")

class Blob():
    def __init__(self, fd, syscall):
        self.fd = fd
        self.syscall = syscall

    def _blob_read(self, offset=None, length=None):
        req = syscalls_pb2.Syscall(readBlob=syscalls_pb2.BlobRead(fd=self.fd, offset=offset, length=length))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.BlobResponse())
        if response.success:
            return response.data
        raise ReadUnnamedError

    def read(self, size=None):
        buf = []
        # if size is unspecified, implementation-dependent
        # faasten now returns at most one block (4K) data
        if size is None:
            return self._blob_read()
        else:
            while size > 0:
                data = self._blob_read(size)
                # reaches EOF
                if len(data) == 0:
                    return buf
                buf.extend(data)
                offset += len(data)
                size -= len(data)
        # size = 0
        return buf

class CreateUnnamedError(Exception):
    pass

class ReadUnnamedError(Exception):
    pass
