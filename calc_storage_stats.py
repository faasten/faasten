#!/usr/bin/env python

from pathlib import Path
import json
import statistics
from collections import defaultdict

d = Path('syscall-storage-stats')
stats = []
for statfile in d.iterdir():
    with open(statfile) as f:
        stats.append(json.loads(f.readline()))

total_num = len(stats)

e2e_lats = defaultdict(list)
for stat in stats:
    latencies = stat['response']['latencies']
    for key in latencies:
        e2e_lats[key].append(latencies[key])

tot_lats = defaultdict(list)
store_lats = defaultdict(list)
serde_lats = defaultdict(list)
label_lats = defaultdict(list)
for stat in stats:
    latencies = stat['syscall']
    for key in latencies:
        tot, store, serde, label = latencies[key]
        tot_lats[key].append(tot['secs']*(10^9) + tot['nanos'])
        store_lats[key].append(store['secs']*(10^9) + store['nanos'])
        serde_lats[key].append(serde['secs']*(10^9) + serde['nanos'])
        label_lats[key].append(label['secs']*(10^9) + label['nanos'])
print('')

header = 'Label store serde label other'
writestoreops = {'write', 'create', 'delete'}
largelines = []
print(header)
for key in tot_lats:
    textkey = '-'.join(key.split('_'))
    store = sum(store_lats[key])/total_num//1000
    serde = sum(serde_lats[key])/total_num//1000
    label = sum(label_lats[key])/total_num//1000
    tot = sum(tot_lats[key])/total_num//1000
    other = tot - store - serde - label
    if key not in storeops:
        line = ' '.join([textkey, str(store), str(serde), str(label), str(other)])
        print(line)
    else:
        line = ' '.join([textkey, str(store/1000), str(serde/1000), str(label/1000), str(other/1000)])
        largelines.append(line)

print('milliseconds')
for l in largelines:
    print(l)
