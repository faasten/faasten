#!/usr/bin/env python

from pathlib import Path
import json
import statistics
from collections import defaultdict

with open('syscall-microbench') as f:
    stat = json.loads(f.readline())
total_num = 1000
nano_to_micro = 1000

e2e_lats = defaultdict(list)
e2e_latencies = stat['response']['latencies']
for key in e2e_latencies:
    e2e_lats[key] = sum(e2e_latencies[key])/total_num/nano_to_micro
print(e2e_lats)

tot_lats = {}
store_lats = {}
serde_lats = {}
label_lats = {}
latencies = stat['syscall']
for key in e2e_latencies:
    tot, store, serde, label = latencies[key]
    tot_lats[key] = (tot['secs']*(10**9) + tot['nanos'])/total_num/nano_to_micro
    store_lats[key] = (store['secs']*(10**9) + store['nanos'])/total_num/nano_to_micro
    serde_lats[key] = (serde['secs']*(10**9) + serde['nanos'])/total_num/nano_to_micro
    label_lats[key] = (label['secs']*(10**9) + label['nanos'])/total_num/nano_to_micro

print(tot_lats)

header = 'Label store serde label other rpc'
writestoreops = {'write', 'create', 'delete'}
largelines = []
print('in microseconds')
print(header)
for key in tot_lats:
    textkey = '-'.join(key.split('_'))
    store = store_lats[key]
    serde = serde_lats[key]
    label = label_lats[key]
    tot = tot_lats[key]
    e2e = e2e_lats[key]
    other = tot - store - serde - label
    rpc = e2e - tot
    line = ' '.join([textkey, str("%.2f"%store), str("%.2f"%serde), str("%.2f"%label), str("%.2f"%other), str("%.2f"%rpc)])
    print(line)
