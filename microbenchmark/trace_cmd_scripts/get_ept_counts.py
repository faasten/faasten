#!/usr/bin/env python3
import os
import print_host_latencies as print_host
from collections import defaultdict

def sortAppName(appname):
    return appname.split('-')[-1] + '-'.join(appname.split('-')[:-1])

styles=['fullapp-eager', 'snapfaas-eager', 'regular']
ept_exit_reason = ['EPT_VIOLATION', 'EPT_MISCONFIG', 'EPT_VIOLATION-mmio', 'HLT']
with open('ept_counts.txt', 'w') as ofile:
    print(','.join(['function name', ','.join(['# of ' + x for x in ept_exit_reason]), ','.join(['mean latency of ' + x for x in ept_exit_reason])]), file=ofile)
    for style in styles:
        round_counts = defaultdict(lambda: defaultdict(list))
        round_sums = defaultdict(lambda: defaultdict(list))
        print(style, file=ofile)
        d = os.path.join('..', style + '-report-out')
        files = os.listdir(d)
        for f in files:
            fpath = os.path.join(d, f)
            with open(fpath) as infile:
                ## discard all lines before the first write to 0x3f0
                #for line in infile:
                #    if '0x3f0' in line:
                #        break
                # only regular has a second write to 0x3f0
                # discard all lines before this second write
                if d == os.path.join('..', styles[-1] + '-report-out'):
                    for line in infile:
                        if '0x3f0' in line:
                            break
                latencies, counts, _, _, _, _ = print_host.parse_report('', infile=infile)
            app = f.split('.')[0]
            for reason in ept_exit_reason:
                round_counts[app][reason].append(counts[reason])
                round_sums[app][reason].append(sum(latencies[reason]))
        for k in sorted(round_counts.keys(), key=sortAppName):
            resstr = ','.join([k, ','.join([str(sum(round_counts[k][reason])/len(round_counts[k][reason])) for reason in ept_exit_reason]),
                ','.join([str(sum(round_sums[k][reason])/sum(round_counts[k][reason])) if sum(round_counts[k][reason]) != 0 else '0' for reason in ept_exit_reason])])
            print(resstr, file=ofile)
