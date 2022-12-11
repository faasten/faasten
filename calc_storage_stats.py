#!/usr/bin/env python

from pathlib import Path
import json
import statistics

d = Path('storage-stats/lsroot')
lsroot = []
for statfile in d.iterdir():
    with open(statfile) as f:
        lsroot.append(json.loads(f.readline()))

d = Path('storage-stats/createuserfile')
createuserfile = []
for statfile in d.iterdir():
    with open(statfile) as f:
        createuserfile.append(json.loads(f.readline()))

d = Path('storage-stats/writeuserfile')
writeuserfile = []
for statfile in d.iterdir():
    with open(statfile) as f:
        writeuserfile.append(json.loads(f.readline()))

d = Path('storage-stats/readuserfile')
readuserfile = []
for statfile in d.iterdir():
    with open(statfile) as f:
        readuserfile.append(json.loads(f.readline()))

d = Path('storage-stats/deleteuserfile')
deleteuserfile = []
for statfile in d.iterdir():
    with open(statfile) as f:
        deleteuserfile.append(json.loads(f.readline()))

print('lines:')
print(len(lsroot))
print(len(createuserfile))
print(len(writeuserfile))
print(len(readuserfile))
print(len(deleteuserfile))
total_num = len(lsroot)

print("Average elapsed (nanos)")
print('lsroot')
elapses = []
for stat in lsroot:
    nanos = stat['elapsed']['secs'] * (10^6) + stat['elapsed']['nanos']
    elapses.append(nanos)
print(sum(elapses)/total_num)
quantiles = statistics.quantiles(elapses)
print(quantiles)

print('createuserfile')
elapses = []
for stat in createuserfile:
    nanos = stat['elapsed']['secs'] * (10^6) + stat['elapsed']['nanos']
    elapses.append(nanos)
print(sum(elapses)/total_num)
quantiles = statistics.quantiles(elapses)
print(quantiles)

print('writeuserfile')
elapses = []
for stat in writeuserfile:
    nanos = stat['elapsed']['secs'] * (10^6) + stat['elapsed']['nanos']
    elapses.append(nanos)
print(sum(elapses)/total_num)
quantiles = statistics.quantiles(elapses)
print(quantiles)

print('readuserfile')
elapses = []
for stat in readuserfile:
    nanos = stat['elapsed']['secs'] * (10^6) + stat['elapsed']['nanos']
    elapses.append(nanos)
print(sum(elapses)/total_num)
quantiles = statistics.quantiles(elapses)
print(quantiles)

print('deleteuserfile')
elapses = []
for stat in deleteuserfile:
    nanos = stat['elapsed']['secs'] * (10^6) + stat['elapsed']['nanos']
    elapses.append(nanos)
print(sum(elapses)/total_num)
quantiles = statistics.quantiles(elapses)
print(quantiles)
