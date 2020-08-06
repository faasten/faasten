#!/usr/bin/env python3
import os
from collections import defaultdict
import sys
import numpy as np

if len(sys.argv) != 2:
    exit(1)

dirname = sys.argv[1]
data = defaultdict(list)
for txt in os.listdir(dirname):
    app = txt.split('.')[0]
    with open(os.path.join(dirname, txt)) as infile:
        lines = infile.readlines()
        boot_time = int(lines[3].split(': ')[-1].split()[0])
        exec_time = int(lines[4].split(': ')[-1].split()[0])
        data[app].append((boot_time, exec_time))

print('app: boot (us), exec (us)')
for app in sorted(data.keys()):
    data[app] = np.sum(data[app], axis=0) / len(data[app])
    print('{}: {}, {}'.format(app, data[app][0], data[app][1]))
