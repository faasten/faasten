from abc import abstractmethod
import syscalls_pb2
import socket
import struct
import json
from google.protobuf.json_format import MessageToJson, _Printer
from contextlib import contextmanager

HttpVerb = syscalls_pb2.HttpVerb

### helper functions ###
def recvall(sock, n):
    # Helper function to recv n bytes or return None if EOF is hit
    data = bytearray()
    while len(data) < n:
        packet = sock.recv(n - len(data))
        if not packet:
            return None
        data.extend(packet)
    return bytes(data)

def split_path(path):
    if not path:
        return [], '', False
    name = path[-1]
    if not isinstance(name, str) or not name:
        return [], '', False
    base = path[:-1]
    return base, name, True

def convert_path(path):
    converted = []
    for comp in path:
        m = syscalls_pb2.PathComponent()
        if isinstance(comp, str):
            m.dscrp = comp
        else:
            m.facet.CopyFrom(comp)
        converted.append(m)
    return converted

### end of helper functions ###

class Response():
    @abstractmethod
    def body_to_bytes(self):
        pass
    @abstractmethod
    def status_code(self):
        pass

class ResponseDict(Response):
    def __init__(self, val, code=200):
        self._val = val
        self._code = code
    def body_to_bytes(self):
        return json.dumps(self._val).encode('utf-8')
    def status_code(self):
        return self._code

class ResponseStr(Response):
    def __init__(self, val, code=200):
        self._val = val
        self._code = code
    def body_to_bytes(self):
        return self._val.encode('utf-8')
    def status_code(self):
        return self._code

class ResponseRaw(Response):
    def __init__(self, val, code=200):
        self._val = val
        self._code = code
    def body_to_bytes(self):
        return self._val
    def status_code(self):
        return self._code

class DummyBackend():
    def sendall(self, bs):
        print(bs)

class DirEntry():
    def __init__(self, fd, syscall):
        self.fd = fd
        self.syscall = syscall

class Directory(DirEntry):
    def ls(self):
        req = syscalls_pb2.Syscall(dentList = self.fd)
        self.syscall._send(req)
        res = self.syscall._recv(syscalls_pb2.DentListResult())
        if res is not None:
            return dict(res.entries)
        else:
            return None

    def link(self, target: DirEntry, name: str):
        return self.syscall.link(self.fd, target.fd, name)

    def unlink(self, name: str):
        req = syscalls_pb2.Syscall(dentUnlink=syscalls_pb2.DentUnlink(fd=self.fd,name=name))
        self.syscall._send(req)
        return self.syscall._recv(syscalls_pb2.DentResult())


    @contextmanager
    def open_at(self, path: [str]):
        cur_dent = self.fd
        cur_kind = syscalls_pb2.DentDirectory
        for component in path:
            v = self.syscall.open_at(cur_dent, component)
            if not v.success:
                raise OpenError
            cur_dent = v.fd
            cur_kind = v.kind
        match cur_kind:
            case syscalls_pb2.DentDirectory:
                yield Directory(cur_dent, self.syscall)
            case syscalls_pb2.DentFile:
                yield File(cur_dent, self.syscall)
            case syscalls_pb2.DentFacetedDirectory:
                yield FacetedDirectory(cur_dent, self.syscall)
            case syscalls_pb2.DentGate:
                yield Gate(cur_dent, self.syscall)
            case syscalls_pb2.DentService:
                yield Service(cur_dent, self.syscall)
            case syscalls_pb2.DentBlob:
                yield BlobEntry(cur_dent, self.syscall)
            case _:
                yield DirEntry(cur_dent, self.syscall)
        req = syscalls_pb2.Syscall(dentClose=cur_dent)
        self.syscall._send(req)
        self.syscall._recv(syscalls_pb2.DentResult())

class File(DirEntry):
    def read(self):
        req = syscalls_pb2.Syscall(dentRead=self.fd)
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.DentResult())
        if response.success:
            return response.data
        else:
            return None

    def write(self, data):
        req = syscalls_pb2.Syscall(dentUpdate=syscalls_pb2.DentUpdate(fd=self.fd, file=data))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.DentResult())
        return response.success

class FacetedDirectory(DirEntry):
    def ls(self):
        req = syscalls_pb2.Syscall(dentLsFaceted = syscalls_pb2.DentLsFaceted(fd = self.fd))
        self.syscall._send(req)
        res = self.syscall._recv(syscalls_pb2.DentLsFacetedResult())
        if res is not None:
            return list(map(_Printer()._MessageToJsonObject, res.facets))
        else:
            return None

class BlobEntry(DirEntry):
    @contextmanager
    def get(self):
        req = syscalls_pb2.Syscall(dentGetBlob=self.fd)
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.BlobResult())
        if response.success:
            yield Blob(response.fd, response.len, self.syscall)
        else:
            raise Exception("No such blob")

class Gate(DirEntry):
    def invoke(self, payload: bytes = b"", sync: bool = True, params: dict[str,str] = {}, toblob: bool = False):
        req = syscalls_pb2.Syscall(dentInvoke=syscalls_pb2.DentInvoke(fd=self.fd, payload=payload, sync=sync, parameters=params, toblob=False))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.DentInvokeResult())
        return response

    def ls(self):
        req = syscalls_pb2.Syscall(dentLsGate=self.fd)
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.DentLsGateResult())
        if response.success:
            return _Printer()._MessageToJsonObject(response.gate)
        else:
            return None

    def update_direct(self, privilege: syscalls_pb2.Component = None,
                            invoker_clearance: syscalls_pb2.Component = None,
                            memory: int = None,
                            app_image: BlobEntry = None,
                            runtime: BlobEntry = None,
                            kernel: BlobEntry = None):
        directGate = syscalls_pb2.DirectGate(
            privilege = privilege,
            invoker_integrity_clearance = invoker_clearance,
            function = syscalls_pb2.Function(
                memory = memory,
                appImage = app_image and app_image.fd,
                runtime = runtime and runtime.fd,
                kernel = kernel and kernel.fd
            )
        )

        req = syscalls_pb2.Syscall(
            dentUpdate = syscalls_pb2.DentUpdate(
                fd = self.fd,
                gate = syscalls_pb2.Gate(direct=directGate)))
        self.syscall._send(req)
        return self.syscall._recv(syscalls_pb2.DentResult())

    def update_redirect(self, privilege: syscalls_pb2.Component = None,
                              invoker_clearance: syscalls_pb2.Component = None,
                              gate = None):
        redirectGate = syscalls_pb2.RedirectGate(
            privilege = privilege,
            invoker_integrity_clearance = invoker_clearance,
            gate = gate and gate.fd
        )

        req = syscalls_pb2.Syscall(
            dentUpdate = syscalls_pb2.DentUpdate(
                fd = self.fd,
                gate = syscalls_pb2.Gate(redirect=redirectGate)))
        self.syscall._send(req)
        return self.syscall._recv(syscalls_pb2.DentResult())


class Service(DirEntry):
    def invoke(self, payload: bytes = b"", sync: bool = True, params: dict[str,str] = {}, toblob: bool = False):
        req = syscalls_pb2.Syscall(dentInvoke=syscalls_pb2.DentInvoke(fd=self.fd, payload=payload, sync=sync, parameters=params, toblob=toblob))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.DentInvokeResult())
        return response


class Blob():
    def __init__(self, fd, length, syscall):
        self.fd = fd
        self.syscall = syscall
        self.length = length
        self.offset = 0

    def _blob_read(self, offset=None, length=None):
        req = syscalls_pb2.Syscall(blobRead=syscalls_pb2.BlobRead(fd=self.fd, offset=offset, length=length))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.BlobResult())
        if response.success:
            return response.data
        raise ReadBlobError

    def read(self, size=None):
        # if size is unspecified, implementation-dependent
        # faasten now returns at most one block (4K) data
        if size is None:
            data = self._blob_read(offset=self.offset)
            self.offset += len(data)
            return data
        else:
            buf = bytearray(b'')
            while size > 0:
                data = self._blob_read(offset=self.offset, length=size)
                # reaches EOF
                if len(data) == 0:
                    return buf
                buf.extend(data)
                self.offset += len(data)
                size -= len(data)
            return buf

    def tell(self):
        return self.offset

    def seek(self, offset, whence=0):
        # TODO handle whence of 2: seek from end of file
        if whence == 1:
            self.offset += offset
        else:
            self.offset = offset

class Syscall():
    def __init__(self, sock):
        self.sock = sock

    def _send(self, obj):
        objData = obj.SerializeToString()
        self.sock.sendall(struct.pack(">I", len(objData)))
        self.sock.sendall(objData)

    def _recv(self, obj):
        data = self.sock.recv(4, socket.MSG_WAITALL)
        res = struct.unpack(">I", data)
        objData = recvall(self.sock, res[0])

        obj.ParseFromString(objData)
        return obj

    def request(self):
        request = syscalls_pb2.Request()
        return self._recv(request)

    def respond(self, resp: Response):
        response = syscalls_pb2.Syscall(response = syscalls_pb2.Response(body = resp.body_to_bytes(), statusCode = resp.status_code()))
        self._send(response)

    def root(self):
        return Directory(0, self)

    def dent_list(self, fd: int):
        """Returns a JSON object"""
        req = syscalls_pb2.Syscall(dentList = fd)
        self._send(req)
        return self._recv(syscalls_pb2.DentListResult())

    def dent_create_dir(self, label: syscalls_pb2.Buckle):
        req = syscalls_pb2.Syscall(
            dentCreate = syscalls_pb2.DentCreate(label = label, directory = syscalls_pb2.Void()))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())

    def dent_create_file(self, label: syscalls_pb2.Buckle):
        req = syscalls_pb2.Syscall(
            dentCreate = syscalls_pb2.DentCreate(label = label, file = syscalls_pb2.Void()))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())

    def dent_create_faceted(self):
        req = syscalls_pb2.Syscall(
            dentCreate = syscalls_pb2.DentCreate(facetedDirectory = syscalls_pb2.Void()))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())

    def dent_create_blob(self, label: syscalls_pb2.Buckle, blobfd: int):
        req = syscalls_pb2.Syscall(
            dentCreate = syscalls_pb2.DentCreate(label = label, blob = blobfd))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())

    def dent_create_direct_gate(self, label: syscalls_pb2.Buckle,
                                privilege: syscalls_pb2.Component,
                                invoker_clearance: syscalls_pb2.Component,
                                memory: int,
                                app_image: BlobEntry,
                                runtime: BlobEntry,
                                kernel: BlobEntry):
        directGate = syscalls_pb2.DirectGate(
            privilege = privilege,
            invoker_integrity_clearance = invoker_clearance,
            function = syscalls_pb2.Function(
                memory = memory,
                appImage = app_image.fd,
                runtime = runtime.fd,
                kernel = kernel.fd
            )
        )

        req = syscalls_pb2.Syscall(
            dentCreate = syscalls_pb2.DentCreate(
                label = label,
                gate = syscalls_pb2.Gate(direct=directGate)))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())

    def dent_create_redirect_gate(self, label: syscalls_pb2.Buckle,
                                  privilege: syscalls_pb2.Component,
                                  invoker_clearance: syscalls_pb2.Component,
                                  gate: Gate):
        redirectGate = syscalls_pb2.RedirectGate(
            privilege = privilege,
            invoker_integrity_clearance = invoker_clearance,
            gate = gate.fd
        )

        req = syscalls_pb2.Syscall(
            dentCreate = syscalls_pb2.DentCreate(
                label = label,
                gate = syscalls_pb2.Gate(redirect=redirectGate)))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())

    def dent_create_service(self, label: syscalls_pb2.Buckle,
                            privilege: syscalls_pb2.Component,
                            invoker_integrity_clearance: syscalls_pb2.Component,
                            taint: syscalls_pb2.Buckle,
                            url: str,
                            verb: int,
                            headers: dict[str,str]):
        service = syscalls_pb2.Service(
            privilege = privilege,
            invoker_integrity_clearance = invoker_integrity_clearance,
            taint = taint,
            url = url,
            verb = verb,
            headers = headers
        )
        req = syscalls_pb2.Syscall(
            dentCreate = syscalls_pb2.DentCreate(
                label = label,
                service = service))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())

    def link(self, dir_fd: int, target_fd: int, name: str):
        req = syscalls_pb2.Syscall(
            dentLink = syscalls_pb2.DentLink(dir_fd = dir_fd, name = name, target_fd = target_fd))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())

    ## Helpers

    def open_at(self, dent: int, name: str) -> syscalls_pb2.DentOpenResult:
        req = syscalls_pb2.Syscall(dentOpen = syscalls_pb2.DentOpen(fd=dent,name=name))
        self._send(req)
        return self._recv(syscalls_pb2.DentOpenResult())

    def open_at_facet(self, dent: int, name: str):
        req = syscalls_pb2.Syscall(dentOpen = syscalls_pb2.DentOpen(fd=dent,name=name))
        self._send(req)
        return self._recv(syscalls_pb2.DentResult())


    ## OLD

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

    def read_dir(self, d):
        d = d.encode('utf-8')
        req = syscalls_pb2.Syscall(readDir = syscalls_pb2.ReadDir(dir = d))
        self._send(req)
        response = self._recv(syscalls_pb2.ReadDirResponse())
        return map(lambda b: b.decode('utf-8'), list(response.keys))

    ### label APIs ###
    def get_current_label(self):
        req = syscalls_pb2.Syscall(getCurrentLabel = syscalls_pb2.Void())
        self._send(req)
        response = self._recv(syscalls_pb2.Buckle())
        return response

    def taint_with_label(self, label):
        req = syscalls_pb2.Syscall(taintWithLabel = label)
        self._send(req)
        response = self._recv(syscalls_pb2.Buckle())
        return response

    def declassify(self, secrecy: syscalls_pb2.Component):
        """Declassify to the target secrecy and leave integrity untouched.
        """
        req = syscalls_pb2.Syscall(declassify = secrecy)
        self._send(req)
        response = self._recv(syscalls_pb2.Buckle())
        return response

    def endorse(self, with_priv: syscalls_pb2.Component=None):
        req = syscalls_pb2.Syscall(endorse = syscalls_pb2.Endorse(withPriv=with_priv))
        self._send(req)
        response = self._recv(syscalls_pb2.DeclassifyResponse())
        return response.label

    def buckle_parse(self, s):
        """ Return a syscalls_pb2.Buckle if string s is valid. Otherwise, None.

        A valid string s has the following format:
        The string separates secrecy and integrity with a comma, clauses
        separated with a '&' and principle vectors with a '|', and delegated
        principles with '/'. The backslash character ('\') allows escaping these
        special characters (including itself).
        """
        req = syscalls_pb2.Syscall(buckleParse = s)
        self._send(req)
        response = self._recv(syscalls_pb2.MaybeBuckle())
        return response.label
    ### end of label APIs ###

    ### gate & privilege ###
    def sub_privilege(self, suffix):
        req = syscalls_pb2.Syscall(subPrivilege = syscalls_pb2.TokenList(tokens = suffix))
        self._send(req)
        response = self._recv(syscalls_pb2.Buckle())
        return response.secrecy

    def dup_gate(self, orig, path, policy):
        base, name, ok = split_path(path)
        if not ok:
            return False
        req = syscalls_pb2.Syscall(dupGate = syscalls_pb2.DupGate(orig = convert_path(orig), baseDir = convert_path(base), name = name, policy = policy))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    ### github APIs ###
    def github_rest_get(self, route, toblob=False):
        req = syscalls_pb2.Syscall(githubRest = syscalls_pb2.GithubRest(verb = syscalls_pb2.HttpVerb.GET, route = route, body = None, toblob=toblob))
        self._send(req)
        response = self._recv(syscalls_pb2.GithubRestResponse())
        return response

    def github_rest_post(self, route, body, toblob=False):
        bodyJson = json.dumps(body)
        req = syscalls_pb2.Syscall(githubRest = syscalls_pb2.GithubRest(verb = syscalls_pb2.HttpVerb.POST, route = route, body = bodyJson, toblob=toblob))
        self._send(req)
        response = self._recv(syscalls_pb2.GithubRestResponse())
        return response

    def github_rest_put(self, route, body, toblob=False):
        bodyJson = json.dumps(body)
        req = syscalls_pb2.Syscall(githubRest = syscalls_pb2.GithubRest(verb = syscalls_pb2.HttpVerb.PUT, route = route, body = bodyJson, toblob=toblob))
        self._send(req)
        response = self._recv(syscalls_pb2.GithubRestResponse())
        return response

    def github_rest_delete(self, route, body, toblob=False):
        bodyJson = json.dumps(body)
        req = syscalls_pb2.Syscall(githubRest = syscalls_pb2.GithubRest(verb = syscalls_pb2.HttpVerb.DELETE, route = route, body = bodyJson, toblob=toblob))
        self._send(req)
        response = self._recv(syscalls_pb2.GithubRestResponse())
        return response
    ### end of github APIs ###

    def invoke(self, gate, payload):
        req = syscalls_pb2.Syscall(invoke = syscalls_pb2.Invoke(gate = convert_path(gate), payload = payload))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def invoke_service(self, service, body):
        bodyJson = json.dumps(body)
        req = syscalls_pb2.Syscall(invokeService = syscalls_pb2.InvokeService(serv = convert_path(service), body = bodyJson))
        self._send(req)
        response = self._recv(syscalls_pb2.ServiceResponse())
        return response

    ###  cloud calls: fs ###
    def fs_hardlink(self, src: str, dest: str):
        """Hard link `src` into `dest`. `src` must be a path naming an object
        that is not gate/service. `dest` must be a path naming a directory
        or a faceted directory. If a faceted directory then the facet named
        by the function's label at the time of linking is used."""
        req = syscalls_pb2.Syscall(fsHardLink = syscalls_pb2.FSHardLink(src=src, dest=dest))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success


    def fs_list(self, path: str):
        """Returns a JSON object"""
        req = syscalls_pb2.Syscall(fsList = syscalls_pb2.FSList(path=path))
        self._send(req)
        response = self._recv(syscalls_pb2.FSListResponse())
        if response.value is not None:
            return json.loads(MessageToJson(response.value))
        return None

    def fs_faceted_list(self, path: str):
        """Returns a JSON object"""
        req = syscalls_pb2.Syscall(fsFacetedList = syscalls_pb2.FSFacetedList(path=path))
        self._send(req)
        response = self._recv(syscalls_pb2.FSFacetedListResponse())
        if response.value is not None:
            return MessageToJson(response.value)
        return None

    def fs_read(self, path: str):
        """Read the file at the `path`.

        Returns:
            bytes: if success
            None: otherwise
        """
        req = syscalls_pb2.Syscall(fsRead = syscalls_pb2.FSRead(path=path))
        self._send(req)
        response = self._recv(syscalls_pb2.ReadKeyResponse())
        return response.value

    @contextmanager
    def fs_openblob(self, path: str):
        """Open the blob at the `path`.

        Returns:
            Blob: if success
            None: otherwise
        """
        req = syscalls_pb2.Syscall(fsOpenBlob = syscalls_pb2.FSOpenBlob(path=path))
        self._send(req)
        response = self._recv(syscalls_pb2.FSOpenBlobResponse())
        if response.name:
            with self.open_blob(response.name) as blob:
                yield blob
        else:
            None

    def fs_write(self, path: str, data):
        """Overwrite the file at the `path` with the `data`.
        The host-side handler always endorse before writing.

        Returns:
            bool: True for success, False otherwise
        """
        req = syscalls_pb2.Syscall(fsWrite = syscalls_pb2.FSWrite(path=path, data=data))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createdir(self, path: str, label: str=None):
        """Create a directory at the `path` with the `label`.
        The host-side handler always endorse before creating the directory.

        Returns:
            bool: True for success, False otherwise
        """
        req = syscalls_pb2.Syscall(fsCreateDir=syscalls_pb2.FSCreateDir(path=path, label=label))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createfile(self, path: str, label: str=None):
        """Create a file at the `path` with the `label`.
        The host-side handler always endorse before creating the file.

        Returns:
            bool: True for success, False otherwise
        """
        req = syscalls_pb2.Syscall(fsCreateFile = syscalls_pb2.FSCreateFile(path=path, label=label))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createfaceted(self, path: str):
        """Create a file at the `path` with the `label`.
        The host-side handler always endorse before creating the file.

        Returns:
            bool: True for success, False otherwise
        """
        req = syscalls_pb2.Syscall(fsCreateFacetedDir = syscalls_pb2.FSCreateFacetedDir(path=path))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_linkblob(self, path, blobname: str, label: str=None):
        """Link `blobname` into the file system at `path`."""
        req = syscalls_pb2.Syscall(fsCreateBlobByName=syscalls_pb2.FSCreateBlobByName(path=path, blobname=blobname, label=label))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_creategate(self, path: str, policy: str, app: str, memory: int, runtime: str):
        req = syscalls_pb2.Syscall(fsCreateGate=syscalls_pb2.FSCreateGate(path=path, policy=policy, appImage=app, memory=memory, runtime=runtime))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createredirectgate(self, path: str, policy: str, redirect_path: str):
        req = syscalls_pb2.Syscall(fsCreateRedirectGate=syscalls_pb2.FSCreateRedirectGate(path=path, policy=policy, redirectPath=redirect_path))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_createservice(self, path: str, policy: str, label: str, url: str, verb: str, headers: str):
        req = syscalls_pb2.Syscall(fsCreateService=syscalls_pb2.FSCreateService(path=path, policy=policy, label=label, url=url, verb=verb, headers=headers))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fs_delete(self, path: str):
        req = syscalls_pb2.Syscall(fsDelete = syscalls_pb2.FSDelete(path=path))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    ### end of named data object syscalls ###

    ### blob data object syscalls #
    @contextmanager
    def create_blob(self, size: int = None):
        """Create a nameless data object.

        The implementation uses the content-addressed blob store.

        Yield:
            An instance of class NewBlob
        """
        req = syscalls_pb2.Syscall(blobCreate=syscalls_pb2.BlobCreate(size=size))
        self._send(req)
        response = self._recv(syscalls_pb2.BlobResult())
        if response.success:
            fd = response.fd
            yield NewBlob(fd, self)
            syscalls_pb2.Syscall(blobClose=syscalls_pb2.BlobClose(fd=fd))
            self._send(req)
            response = self._recv(syscalls_pb2.BlobResult())
        else:
            raise CreateBlobError

    @contextmanager
    def open_blob(self, name):
        """Open a content-addressed immutable data object.

        Yield:
            An instance of class Blob
        """
        req = syscalls_pb2.Syscall(openBlob=syscalls_pb2.BlobOpen(name=name))
        self._send(req)
        response = self._recv(syscalls_pb2.BlobResult())
        fd = response.fd
        yield Blob(fd, self)
        req = syscalls_pb2.Syscall(closeBlob=syscalls_pb2.BlobClose(fd=fd))
        self._send(req)
        response = self._recv(syscalls_pb2.BlobResult())
    ### end of unnamed object syscalls ###

    ### return direntry handle ###
    @contextmanager
    def create_file(self, path: str, label: str=None):
        req = syscalls_pb2.Syscall(createFile = syscalls_pb2.CreateFile(path=path, label=label))
        self._send(req)
        response = self._recv(syscalls_pb2.DentResponse())
        if response.success:
            yield File(response.dentFd, self)
            req = syscalls_pb2.Syscall(dentClose=syscalls_pb2.DentClose(dentFd=response.dentFd))
            self._send(req)
        else:
            raise CreateFileError

class NewBlob():
    def __init__(self, fd, syscall):
        self.fd = fd
        self.syscall = syscall

    def write(self, data):
        req = syscalls_pb2.Syscall(blobWrite=syscalls_pb2.BlobWrite(fd=self.fd, data=data))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.BlobResult())
        return response.success

    def finalize(self, data):
        req = syscalls_pb2.Syscall(blobFinalize=syscalls_pb2.BlobFinalize(fd=self.fd, data=data))
        self.syscall._send(req)
        response = self.syscall._recv(syscalls_pb2.BlobResult())
        return response.data.decode("utf-8")

class CreateBlobError(Exception):
    pass

class ReadBlobError(Exception):
    pass

class CreateFileError(Exception):
    pass

class OpenError(Exception):
    pass
