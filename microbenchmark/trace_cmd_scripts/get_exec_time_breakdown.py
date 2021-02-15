#!/usr/bin/env python3
import os
import print_host_latencies as print_host
from collections import defaultdict
import sys

if len(sys.argv) != 2:
    print('usage: python3 get_exec_time_breakdown.py REPORT_DIRECTORY')
    sys.exit(1)

report_directory = sys.argv[1]

def sortAppName(appname):
    return appname.split('-')[-1] + '-'.join(appname.split('-')[:-1])
#get latencies
#latency_dirs=['../fullapp-eager-latency-out',
#        '../snapfaas-eager-latency-out',
#        '../regular-latency-out']

#exec_latencies_all = []
#for d in latency_dirs:
#    files = sorted(os.listdir(d))
#    exec_latencies = {}
#    for f in files:
#        fpath = os.path.join(d, f)
#        with open(fpath) as infile:
#            lines = infile.readlines()
#            app = f.split('.')[0]
#            # in microseconds
#            exec_latencies[app] = int(lines[4].split()[-2])
#    exec_latencies_all.append(exec_latencies)

styles=['fullapp-eager', 'snapfaas-eager', 'regular']
exit_reason_ordering = ['EPT_VIOLATION', 'EPT_MISCONFIG', 'EPT_VIOLATION-mmio', 'HLT', 'EXTERNAL_INTERRUPT', 'PREEMPTION_TIMER', 'MSR_WRITE']
with open('breakdown.raw.txt', 'w') as ofile:
    print(','.join(['function name', ','.join(exit_reason_ordering)]), file=ofile)
    for style in styles:
        round_latencies = defaultdict(lambda: defaultdict(list))
        print(style, file=ofile)
        d = os.path.join(report_directory, style + '-report-out')
        files = os.listdir(d)
        for f in files:
            fpath = os.path.join(d, f)
            with open(fpath) as infile:
                ## discard all lines before the first write to 0x3f0
                #for line in infile:
                #    if '0x3f0' in line:
                #        break
                # only regular has a second write tto 0x3f0
                # discard all lines before this second write
                if style == styles[-1]:
                    for line in infile:
                        if '0x3f0' in line:
                            break
                latencies, _, _, _, _, _ = print_host.parse_report('', infile=infile)
            app = f.split('.')[0]
            for r, l in latencies.items():
                round_latencies[app][r].append(sum(l))
        for k in sorted(round_latencies.keys(), key=sortAppName):
            num_rounds = len(round_latencies[k]['EPT_MISCONFIG'])
            resstr = ','.join([k, ','.join([str(sum(round_latencies[k][reason])/num_rounds) for reason in exit_reason_ordering])])
            for r in sorted(round_latencies[k].keys()):
                if r not in exit_reason_ordering:
                    resstr = ','.join([resstr, r, str(sum(round_latencies[k][r])/num_rounds)])
            print(resstr, file=ofile)
