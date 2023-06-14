#!/usr/bin/env python3

from importlib import import_module
import json
import time
import socket
import sys
import traceback
import faasten

app = import_module('workload')

VSOCKPORT = 1234
faasten.vsock(VSOCKPORT)
while True:
    try:
        request = faasten.syscall.request()

        start = time.monotonic_ns()
        # return value from Lambda can be not JSON serializable
        response = app.handle(json.loads(request.payload))
        response['duration'] = time.monotonic_ns() - start
        faasten.syscall.respond(response)
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
        faasten.syscall.respond(response)
