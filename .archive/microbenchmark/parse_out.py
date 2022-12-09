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
            json_time = int(lines[0].split(': ')[-1].split()[0])
            preconfig_time = int(lines[1].split(': ')[-1].split()[0])
            boot_incl_preconfig = int(lines[5].split(': ')[-1].split()[0])
            boot_time = boot_incl_preconfig - preconfig_time
            exec_time = int(lines[6].split(': ')[-1].split()[0])
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
    # print(dir)

base_version = 'snapfaas-reap'
base_boot = np.array(boot_versions[base_version])
base_exec = np.array(exec_versions[base_version])
base_e2e = base_boot + base_exec

normalized_boot = {}
for ver in boot_versions.keys():
    if ver != base_version:
        normalized_boot[ver] = np.array(boot_versions[ver]) / base_boot

normalized_exec = {}
for ver in exec_versions.keys():
    if ver != base_version:
        normalized_exec[ver] = np.array(exec_versions[ver]) / base_exec

normalized_e2e = {}
for ver in exec_versions.keys():
    if ver != base_version:
        normalized_e2e[ver] = (np.array(exec_versions[ver]) + np.array(boot_versions[ver])) / base_exec

cols = ['normalized boot']
cols.extend(sorted(data.keys()))
print(','.join(cols))
for ver in normalized_boot.keys():
    cols = [ver]
    cols.extend(map(str, normalized_boot[ver]))
    print(','.join(cols))

cols = ['normalized exec']
cols.extend(sorted(data.keys()))
print(','.join(cols))
for ver in normalized_exec.keys():
    cols = [ver]
    cols.extend(map(str, normalized_exec[ver]))
    print(','.join(cols))

cols = ['normalized e2e']
cols.extend(sorted(data.keys()))
print(','.join(cols))
for ver in normalized_e2e.keys():
    cols = [ver]
    cols.extend(map(str, normalized_e2e[ver]))
    print(','.join(cols))
