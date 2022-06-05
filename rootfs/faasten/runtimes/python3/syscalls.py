import syscalls_pb2
import socket
import struct
import json

def recvall(sock, n):
    # Helper function to recv n bytes or return None if EOF is hit
    data = bytearray()
    while len(data) < n:
        packet = sock.recv(n - len(data))
        if not packet:
            return None
        data.extend(packet)
    return data

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

    def invoke(self, function, payload):
        req = syscalls_pb2.Syscall(invoke = syscalls_pb2.Invoke(function = function, payload = payload))
        self._send(req)
        response= self._recv(syscalls_pb2.InvokeResponse())
        return response.success

    def fsread(self, path):
        req = syscalls_pb2.Syscall(fsRead = syscalls_pb2.FSRead(path = path))
        self._send(req)
        response = self._recv(syscalls_pb2.ReadKeyResponse())
        return response.value

    def fswrite(self, path, data):
        req = syscalls_pb2.Syscall(fsWrite = syscalls_pb2.FSWrite(path = path, data = data))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fscreate_dir(self, path, name, label):
        req = syscalls_pb2.Syscall(fsCreateDir = syscalls_pb2.FSCreateDir(baseDir = path, name = name, label = label))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def fscreate_file(self, path, name, label):
        req = syscalls_pb2.Syscall(fsCreateFile = syscalls_pb2.FSCreateFile(baseDir = path, name = name, label = label))
        self._send(req)
        response = self._recv(syscalls_pb2.WriteKeyResponse())
        return response.success

    def endorse_with(self, clauses):
        cur_label = self.get_current_label()
        target = syscalls_pb2.DcLabel()
        target.secrecy.CopyFrom(cur_label.secrecy)
        target.integrity.CopyFrom(cur_label.integrity)
        for c in clauses:
            target.integrity.clauses.add().principals.extend(c)
        return self.exercise_privilege(target)

    def declassify_to(self, secrecy):
        cur_label = self.get_current_label()
        target = syscalls_pb2.DcLabel()
        for c in secrecy:
            target.secrecy.clauses.add().principals.extend(c)
        target.integrity.CopyFrom(cur_label.integrity)
        return self.exercise_privilege(target)

    def exercise_privilege(self, label):
        req = syscalls_pb2.Syscall(exercisePrivilege = label)
        self._send(req)
        response = self._recv(syscalls_pb2.DcLabel())
        return response

    @staticmethod
    def new_dclabel(secrecy, integrity):
        label = syscalls_pb2.DcLabel()
        for c in secrecy:
            label.secrecy.clauses.add().principals.extend(c)
        for c in integrity:
            label.integrity.clauses.add().principals.extend(c)
        return label
