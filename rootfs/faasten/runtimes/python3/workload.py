#!/usr/bin/env python3

from importlib import import_module
import json
import time
import socket
import os
import sys
import traceback
from subprocess import run, Popen
from syscalls import Syscall

# vsock to communicate with the host
VSOCKPORT = 1234
sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
hostaddr = (socket.VMADDR_CID_HOST, VSOCKPORT)

app = import_module('workload')

sock.connect(hostaddr)
sc = Syscall(sock)
while True:
    request = sc.request()

    start = time.monotonic_ns()
    try:
        # return value from Lambda can be not JSON serializable
        response = app.handle(json.loads(request.payload), request.dataHandles, sc)
        response['duration'] = time.monotonic_ns() - start
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
        response['duration'] = time.monotonic_ns() - start
        sc.respond(response)
