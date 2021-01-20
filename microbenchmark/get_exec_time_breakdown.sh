#!/usr/bin/env bash

source ./default_env

[ ! -d ept_out ] && mkdir ept_out
for (( i=0; i<100; i++ ))
do
    echo 'Round' $i
    >ept_out/ept_$i.txt
    for runtime in python3 nodejs
    do
        for app in $(ls ../snapfaas-images/appfs/$runtime)
        do
            cat ../resources/requests/$app-$runtime.json | head -1 > tmp.json

            echo "- full-function-eager: $app-$runtime" | tee -a ept_out/ept_$i.txt
            sudo trace-cmd record -e kvm \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $SSDROOTFSDIR/$app-$runtime.ext4 \
                --load_dir $SSDSNAPSHOTDIR/$app-$runtime \
                --copy_base \
                --odirect_base &>/dev/null < tmp.json
            trace-cmd report 1> ept_report 2>/dev/null
            trace_cmd_scripts/print_host_latencies.py --ftrace_report ept_report > tmp.out
            total1=$(cat tmp.out | head -2 | tail -1 | awk '{ print $2 }')
            echo "total(us): $total1" >> ept_out/ept_$i.txt
            count1=$(cat tmp.out | head -2 | tail -1 | awk '{ print $3 }')
            echo "count: $count1" >> ept_out/ept_$i.txt

            echo "- regular boot: $app-$runtime" | tee -a ept_out/ept_$i.txt
            sudo trace-cmd record -e kvm \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $SSDROOTFSDIR/$app-$runtime.ext4 &>/dev/null < tmp.json
            trace-cmd report 1>ept_report 2>/dev/null
            # find the line of port IO write 124 to 0x3f0
            linenum=$(egrep -n 0x3f0 ept_report | head -1 | awk -F: '{ print $1 }')
            # discard all traces up to the port IO
            cat ept_report | tail -n +$((linenum + 1)) > ept_report_truncated
            trace_cmd_scripts/print_host_latencies.py --ftrace_report ept_report_truncated > tmp.out
            total2=$(cat tmp.out | head -2 | tail -1 | awk '{ print $2 }')
            echo "total(us): $total2" >> ept_out/ept_$i.txt
            count2=$(cat tmp.out | head -2 | tail -1 | awk '{ print $3 }')
            echo "count: $count2" >> ept_out/ept_$i.txt

            echo "- difference" >> ept_out/ept_$i.txt
            echo "total(us): $((total1-total2))" >> ept_out/ept_$i.txt
            echo "count: $((count1-count2))" >> ept_out/ept_$i.txt
        done
    done
done

rm ept_report* tmp.*
rm -f trace.dat
