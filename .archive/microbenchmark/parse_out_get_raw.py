#!/usr/bin/env python3
import os
from collections import defaultdict
import sys
import numpy as np

dirs = os.listdir('./out')
boot_versions = {}
exec_versions = {}

filtered_app = {'tpcc', 'hello'}
for dir in dirs:
    data = defaultdict(list)
    for txt in os.listdir(os.path.join('./out', dir)):
        app = txt.split('.')[0]
        if app == 'tpcc-java':
            continue
        if 'hello' in app:
            continue
        with open(os.path.join('./out', dir, txt)) as infile:
            lines = infile.readlines()
            memory_restore_time = int(lines[0].split(': ')[-1].split()[0])
            json_time = int(lines[1].split(': ')[-1].split()[0])
            preconfig_time = int(lines[2].split(': ')[-1].split()[0])
            boot_incl_preconfig = int(lines[6].split(': ')[-1].split()[0])
            boot_time = boot_incl_preconfig - preconfig_time
            exec_time = int(lines[7].split(': ')[-1].split()[0])
            data[app].append((boot_time, exec_time))

    boot_data = []
    exec_data = []
    for app in sorted(data.keys()):
        data[app] = np.sum(data[app], axis=0) / len(data[app])
        boot_data.append(data[app][0])
        exec_data.append(data[app][1])

    ver = '-'.join(dir.split('-')[:-3])
    boot_versions[ver] = boot_data
    exec_versions[ver] = exec_data

cols = ['boot latency (us)']
cols.extend(sorted(data.keys()))
print(','.join(cols))
for ver in boot_versions.keys():
    cols = [ver]
    cols.extend(map(str, boot_versions[ver]))
    print(','.join(cols))

cols = ['execution latency (us)']
cols.extend(sorted(data.keys()))
print(','.join(cols))
for ver in exec_versions.keys():
    cols = [ver]
    cols.extend(map(str, exec_versions[ver]))
    print(','.join(cols))

cols = ['normalized e2e']
cols.extend(sorted(data.keys()))
print(','.join(cols))
for ver in boot_versions.keys():
    cols = [ver]
    cols.extend(map(str, np.array(exec_versions[ver]) + np.array(boot_versions[ver])))
    print(','.join(cols))
