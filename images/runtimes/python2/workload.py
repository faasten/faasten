#!/usr/bin/env python2

import sys
import os
import imp
import struct
import json
from subprocess import call, Popen
import multiprocessing as mp
import time

# for snapshot
for i in range(1, mp.cpu_count()):
    Popen('taskset -c %d outl 124 0x3f0'%(i), shell=True)
call('taskset -c 0 outl 124 0x3f0', shell=True)

os.system("mount -r /dev/vdb /srv")

with open('/dev/ttyS1', 'r') as tty, open('/dev/ttyS1', 'w') as out:
    # signal firerunner we are ready
    call('outl 126 0x3f0', shell=True)

    sys.path.append('/srv/package')
    app = imp.load_source('app', '/srv/workload')

    while True:
        request = json.loads(tty.readline())
        t0 = time.clock()

        response = app.handle(request)
        t1 = time.clock()
        response['runtime'] = (t1-t0) * 1000

        responseJson = json.dumps(response)

        out.write(struct.pack(">I", len(responseJson)))
        out.write(bytes(responseJson))
        out.flush()

