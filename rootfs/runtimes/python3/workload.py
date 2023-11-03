#!/usr/bin/env python3

from importlib import import_module
import json
import socket
import sys
import traceback
from syscalls import Syscall, Response, ResponseDict

# vsock to communicate with the host
VSOCKPORT = 1234
sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
hostaddr = (socket.VMADDR_CID_HOST, VSOCKPORT)

app = import_module('workload')

sock.connect(hostaddr)
sc = Syscall(sock)
while True:
    try:
        request = sc.request()
        response = app.handle(sc, payload=request.payload, blobs=request.blobs, headers=request.headers, invoker=request.invoker)
        assert(isinstance(response, Response))
        sc.respond(response)
    except:
        ty, val, tb = sys.exc_info()
        response = {
            'error': {
                'type': str(ty),
                'value': str(val),
                'traceback': traceback.format_tb(tb),
            },
        }
        sc.respond(ResponseDict(response))
