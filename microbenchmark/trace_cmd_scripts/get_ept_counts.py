#!/usr/bin/env python3
import os
import print_host_latencies as print_host

styles=['fullapp-eager', 'snapfaas-eager', 'regular']
ept_exit_reason = ['EPT_VIOLATION', 'EPT_MISCONFIG', 'EPT_VIOLATION-mmio']
with open('ept_counts.txt', 'w') as ofile:
    print(','.join(['function name', '# of EPT_VIOLATION', '# of EPT_MISCONFIG', '# of EPT_VIOLATION-mmio', 'mean latency of EPT_VIOLATION', 'mean latency of EPT_MISCONFIG',
        'mean latency of EPT_VIOLATION-mmio']), file=ofile)
    for style in styles:
        print(style, file=ofile)
        d = os.path.join('..', style + '-report-out')
        files = sorted(os.listdir(d), key=lambda x: x.split('-')[-1] + '-'.join(x.split('-')[:-1]))
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
            total_count = 0
            resstr = ','.join([app, ','.join([str(counts[reason]) for reason in ept_exit_reason]),
                ','.join([str(sum(latencies[reason])/counts[reason]) if counts[reason] != 0 else '0' for reason in ept_exit_reason])])
            print(resstr, file=ofile)
