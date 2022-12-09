#!/usr/bin/env bash

if [ $# -ne 3 ]; then
    echo 'usage: ./get_exec_time_breakdown.sh START_ROUND END_ROUND(inclusive) OUTPUT_DIRECTORY'
    exit 1
fi

OUTPUT_DIRECTORY=$3
if [ ! -d $OUTPUT_DIRECOTRY ]; then
    echo "error: $OUTPUT_DIRECTORY does not exist"
    exit 1
fi
FULLAPP_LAT_OUT=$OUTPUT_DIRECTORY/fullapp-eager-latency-out
SNAPFAAS_LAT_OUT=$OUTPUT_DIRECTORY/snapfaas-eager-latency-out
REGULAR_LAT_OUT=$OUTPUT_DIRECTORY/regular-latency-out

FULLAPP_REPORT_OUT=$OUTPUT_DIRECTORY/fullapp-eager-report-out
SNAPFAAS_REPORT_OUT=$OUTPUT_DIRECTORY/snapfaas-eager-report-out
REGULAR_REPORT_OUT=$OUTPUT_DIRECTORY/regular-report-out

source ./default_env

# create directories for latency numbers
[ ! -d $FULLAPP_LAT_OUT ]  &&  mkdir $FULLAPP_LAT_OUT
[ ! -d $SNAPFAAS_LAT_OUT ] &&  mkdir $SNAPFAAS_LAT_OUT
[ ! -d $REGULAR_LAT_OUT ]  &&  mkdir $REGULAR_LAT_OUT
# create directories for ftrace reports
[ ! -d $FULLAPP_REPORT_OUT ] && mkdir  $FULLAPP_REPORT_OUT
[ ! -d $SNAPFAAS_REPORT_OUT ] && mkdir $SNAPFAAS_REPORT_OUT
[ ! -d $REGULAR_REPORT_OUT ] && mkdir  $REGULAR_REPORT_OUT

for (( i=$1; i<=$2; i++ ))
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
                --odirect_base 1>$FULLAPP_LAT_OUT/$app-$runtime.$i.txt 2>/dev/null < <(cat ../resources/requests/$app-$runtime.json | head -1)
            trace-cmd report 1> $FULLAPP_REPORT_OUT/$app-$runtime.$i.txt 2>/dev/null
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
                --copy_diff 1>$SNAPFAAS_LAT_OUT/$app-$runtime.$i.txt 2>/dev/null < <(cat ../resources/requests/$app-$runtime.json | head -1)
            trace-cmd report 1> $SNAPFAAS_REPORT_OUT/$app-$runtime.$i.txt 2>/dev/null

            echo "- regular boot: $app-$runtime"
            sudo trace-cmd record -e kvm \
            taskset -c 0 sudo $MEMBINDIR/fc_wrapper \
                --vcpu_count 1 \
                --mem_size 128 \
                --kernel $KERNEL \
                --network $NETDEV \
                --firerunner $MEMBINDIR/firerunner \
                --rootfs $SSDROOTFSDIR/regular/$app-$runtime.ext4 \
                1>$REGULAR_LAT_OUT/$app-$runtime.$i.txt 2>/dev/null < <(cat ../resources/requests/$app-$runtime.json | head -1)
            trace-cmd report 1>$REGULAR_REPORT_OUT/$app-$runtime.$i.txt 2>/dev/null
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
