for mem in 10
do
    echo "starting snapctr with $mem GB of memory"
    let "mem_mib = $mem*1024"
    echo "snapctr memory size: $mem_mib"

    echo "launching snapctr"
    ./target/release/snapctr -p 28888 --mem $mem_mib > snapctr-log-$mem.log &
    echo "starting client ..."
    sleep 2
    ./target/release/client -s localhost:28888 -i experiments/multiapp-medium.json

    if [ $? -eq 0 ]
    then
        echo "client exited without error"
        echo "killing snapctr"
        sudo kill -s SIGINT $(pidof snapctr)
        sleep 2
        mkdir -p experiments/results/$mem-gb
        mv out/*.stat experiments/results/$mem-gb
    else
        echo "client exited with error"
        sudo kill -s SIGINT $(pidof snapctr)
        sleep 2
        rm out/*.stat
    fi

    sleep 5

done
