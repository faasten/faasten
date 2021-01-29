#!/usr/bin/env python3
import os
import subprocess
import print_host_latencies as print_host

#get latencies
latency_dirs=['../fullapp-eager-latency-out',
        '../snapfaas-eager-latency-out',
        '../regular-latency-out']

exec_latencies_all = []
for d in latency_dirs:
    files = sorted(os.listdir(d))
    exec_latencies = {}
    for f in files:
        fpath = os.path.join(d, f)
        with open(fpath) as infile:
            lines = infile.readlines()
            app = f.split('.')[0]
            # in microseconds
            exec_latencies[app] = int(lines[4].split()[-2])
    exec_latencies_all.append(exec_latencies)

report_dirs=['../fullapp-eager-report-out',
        '../snapfaas-eager-report-out',
        '../regular-report-out']
exit_reason_ordering = ['EPT_VIOLATION', 'EPT_MISCONFIG', 'EPT_VIOLATION-mmio', 'HLT', 'EXTERNAL_INTERRUPT', 'PREEMPTION_TIMER', 'MSR_WRITE']
with open('breakdown.raw.txt', 'w') as ofile, open('breakdown.txt', 'w') as ofile2:
    for d, exec_latencies in zip(report_dirs, exec_latencies_all):
        print(d, file=ofile)
        print(d, file=ofile2)
        files = os.listdir(d)
        for f in files:
            fpath = os.path.join(d, f)
            with open(fpath) as infile:
                # discard all lines before the first write to 0x3f0
                for line in infile:
                    if '0x3f0' in line:
                        break
                # only regular has a second write tto 0x3f0
                # discard all lines before this second write
                if d == report_dirs[-1]:
                    for line in infile:
                        if '0x3f0' in line:
                            break
                latencies, counts, _, _, _, _ = print_host.parse_report('', infile=infile)
            app = f.split('.')[0]
            iotypes = ['EPT_VIOLATION-mmio','EPT_MISCONFIG','HLT']
            iototal = 0
            for t in iotypes:
                iototal += sum(latencies[t])
            resstr = ','.join([app, 'total', str(exec_latencies[app]), 'I/O', str(iototal), 'page faults', str(sum(latencies['EPT_VIOLATION']))])
            for k, v in sorted(latencies.items(), key=lambda x: sum(x[1]), reverse=True):
                if k not in iotypes and k != 'EPT_VIOLATION':
                    resstr = ','.join([resstr, k, str(sum(v))])
            print(resstr, file=ofile2)

            resstr = ','.join([app, 'total', str(exec_latencies[app])])
            for reason in exit_reason_ordering:
                resstr = ','.join([resstr, reason, str(sum(latencies[reason]))])
            for k, v in sorted(latencies.items(), key=lambda x: x[0]):
                if k not in exit_reason_ordering:
                    resstr = ','.join([resstr, k, str(sum(v))])
            print(resstr, file=ofile)
