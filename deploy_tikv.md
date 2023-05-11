* Pull docker images
```sh
docker pull pingcap/tikv:latest
docker pull pingcap/pd:latest
```
* Environment var
  * IP=10.10.1.2
  * DATA_DIR=/mnt/disk1/tikv/data

* Start pd
```sh
docker run -d --name pd1 \
-p 2379:2379 \
-p 2380:2380 \
-v /etc/localtime:/etc/localtime:ro \
-v $DATA_DIR:/data \
pingcap/pd:latest \
--name="pd1" \
--data-dir="/data/pd1" \
--client-urls="http://0.0.0.0:2379" \
--advertise-client-urls="http://$IP:2379" \
--peer-urls="http://0.0.0.0:2380" \
--advertise-peer-urls="http://$IP:2380" \
--initial-cluster="pd1=http://$IP:2380"
```

* Start tikv server
```sh
docker run -d --name tikv1 \
-p 20160:20160 \
-v /etc/localtime:/etc/localtime:ro \
-v $DATA_DIR:/data \
pingcap/tikv:latest \
--addr="0.0.0.0:20160" \
--advertise-addr="$IP:20160" \
--data-dir="/data/tikv1" \
--pd="$IP:2379"
```

* check if working
```sh
curl $IP:2379/pd/api/v1/stores
```
