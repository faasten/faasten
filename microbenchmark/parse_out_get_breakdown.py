#!/usr/bin/env python3
import os
from collections import defaultdict
import sys
import numpy as np

out_dir = './new_out'
dirs = os.listdir(out_dir)
versions = {}

import csv
regular_execs = {}
rows = []
with open('regular.csv') as f:
    input_csv = csv.reader(f)
    for row in input_csv:
        rows.append(row)
for i, name in enumerate(rows[0]):
    regular_execs[name] = float(rows[1][i])
# print(regular_execs)
for dir in dirs:
    data = defaultdict(list)
    for txt in os.listdir(os.path.join(out_dir, dir)):
        app = txt.split('.')[0]
        with open(os.path.join(out_dir, dir, txt)) as infile:
            lines = infile.readlines()
            
            # mem
            memory_restore = int(lines[0].split(': ')[-1].split()[0])
            
            # c
            tot_restore_time = int(lines[3].split(': ')[-1].split()[0])
            other_restore_time =  tot_restore_time - memory_restore
            json_time = int(lines[1].split(': ')[-1].split()[0])
            preconfig_time = int(lines[2].split(': ')[-1].split()[0])
            constant = json_time + preconfig_time + other_restore_time

            # remaining initialization
            boot_incl_preconfig = int(lines[6].split(': ')[-1].split()[0])
            remaining_init = boot_incl_preconfig - json_time - preconfig_time - tot_restore_time

            # exec
            exec_time = int(lines[7].split(': ')[-1].split()[0]) 

            data[app].append([constant, memory_restore, remaining_init, exec_time])

    for app in sorted(data.keys()):
        data[app] = np.sum(data[app], axis=0) / len(data[app])
        data[app][-1] = data[app][-1] - regular_execs[app] * 1000

    ver = '-'.join(dir.split('-')[:-3])
    versions[ver] = data

# version1 version2 version3 version4
headers1 = ',' + ','.join(map(lambda s: s+',,,', sorted(versions.keys())))
# constant || memory_restore || remaining_init || exec_time
headers2 = ',' + ','.join(['constant,memory_restore,remaining_init,exec']*4)
# name, (data, data, data ,data) * 4
print(headers1)
print(headers2)
lang = ''
for app in sorted(data.keys(), key=lambda x: x.split('-')[-1] + '-'.join(x.split('-')[:-1])):
    row = []
    this_lang = app.split('-')[-1]
    if this_lang != lang:
        lang = this_lang
        print(lang+','*16)
    for ver in sorted(versions.keys()):
        row.extend(versions[ver][app])
    row = ','.join(['-'.join(app.split('-')[:-1]), ','.join(map(lambda x: str(round(x, 1)), np.array(row)/1000))])
    print(row)
