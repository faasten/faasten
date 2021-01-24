#!/usr/bin/env bash

source ./default_env

# create directories for latency numbers
[ ! -d fullapp-eager-latency-out ] && mkdir fullapp-eager-latency-out
[ ! -d snapfaas-eager-latency-out ] && mkdir snapfaas-eager-latency-out
[ ! -d regular-latency-out ] && mkdir regular-latency-out
# create directories for ftrace reports
[ ! -d fullapp-eager-report-out ] && mkdir fullapp-eager-report-out
[ ! -d snapfaas-eager-report-out ] && mkdir snapfaas-eager-report-out
[ ! -d regular-report-out ] && mkdir regular-report-out
for (( i=0; i<1; i++ ))
do
    echo 'Round' $i
    for runtime in python3 nodejs
    do
        for app in $(ls ../snapfaas-images/appfs/$runtime)
        do
            echo "- full-function-eager: $app-$runtime"
            sudo trace-cmd record -e kvm \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $SSDROOTFSDIR/fullapp/$app-$runtime.ext4 \
                --load_dir $SSDSNAPSHOTDIR/$app-$runtime \
                --copy_base \
                --odirect_base 1>fullapp-eager-latency-out/$app-$runtime.$i.txt 2>/dev/null < <(cat ../resources/requests/$app-$runtime.json | head -1)
            trace-cmd report 1> fullapp-eager-report-out/$app-$runtime.$i.txt 2>/dev/null
            #trace_cmd_scripts/print_host_latencies.py --ftrace_report ept_report > breakdown.$1.txt
            #total1=$(cat tmp.out | head -2 | tail -1 | awk '{ print $2 }')
            #echo "total(us): $total1" >> ept_out/ept_$i.txt
            #count1=$(cat tmp.out | head -2 | tail -1 | awk '{ print $3 }')
            #echo "count: $count1" >> ept_out/ept_$i.txt
            echo "- snapfaas-eager: $app-$runtime"
            sudo trace-cmd record -e kvm \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $SSDROOTFSDIR/snapfaas/$runtime.ext4 \
                --appfs $SSDAPPFSDIR/$app-$runtime.ext2 \
                --load_dir $MEMSNAPSHOTDIR/$runtime \
                --diff_dirs $SSDSNAPSHOTDIR/diff/$app-$runtime \
                --copy_diff 1>snapfaas-eager-latency-out/$app-$runtime.$i.txt 2>/dev/null < <(cat ../resources/requests/$app-$runtime.json | head -1)
            trace-cmd report 1> snapfaas-eager-report-out/$app-$runtime.$i.txt 2>/dev/null

            echo "- regular boot: $app-$runtime"
            sudo trace-cmd record -e kvm \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $SSDROOTFSDIR/regular/$app-$runtime.ext4 1>regular-latency-out/$app-$runtime.$i.txt 2>/dev/null < <(cat ../resources/requests/$app-$runtime.json | head -1)
            trace-cmd report 1>regular-report-out/$app-$runtime.$i.txt 2>/dev/null
            # find the line of port IO write to 0x3f0
            #linenum=$(egrep -n 0x3f0 ept_report | head -1 | awk -F: '{ print $1 }')
            # discard all traces up to the port IO
            #cat ept_report | tail -n +$((linenum + 1)) > regular-report-out/$app-$runtime.$i.txt
            #trace_cmd_scripts/print_host_latencies.py --ftrace_report ept_report_truncated > tmp.out
            #total2=$(cat tmp.out | head -2 | tail -1 | awk '{ print $2 }')
            #echo "total(us): $total2" >> ept_out/ept_$i.txt
            #count2=$(cat tmp.out | head -2 | tail -1 | awk '{ print $3 }')
            #echo "count: $count2" >> ept_out/ept_$i.txt

            #echo "- difference" >> ept_out/ept_$i.txt
            #echo "total(us): $((total1-total2))" >> ept_out/ept_$i.txt
            #echo "count: $((count1-count2))" >> ept_out/ept_$i.txt
        done
    done
done
