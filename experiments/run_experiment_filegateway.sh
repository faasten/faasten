for mem in {5..160..5}
do
    echo "starting snapctr with $mem GB of memory"
    let "mem_mib = $mem*1024"
    echo "$mem_mib"
    ./target/release/snapctr --requests_file experiments/multiapp-medium.json --mem $mem_mib > /dev/null
    if [ $? -eq 0 ]
    then
        echo "snapctr exited without error"
        mkdir -p experiments/results/$mem-gb
        mv out/*.stat experiments/results/$mem-gb
    else
        echo "snapctr exited with error"
    fi

    sleep 5

done
