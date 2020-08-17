# How to generate workload files

`generator.py` generates synthetic workloads for SnapFaaS controller in the form of JSON files.
You can try an example by running
```bash
python3 generator.py multiapp-medium/workload.yaml > multiapp-medium.json
```

The workload files consists JSON strings of the form:
```json
{"payload": {"request": 42}, "user_id": 130, "time": 5, "function": "markdown-to-html"}
{"payload": {"request": 42}, "user_id": 16, "time": 10, "function": "sentiment-analysis"}
{"payload": {"request": 42}, "user_id": 84, "time": 11, "function": "markdown-to-html"}
{"payload": {"request": 42}, "user_id": 10, "time": 24, "function": "ocr-img"}
```
where `time` is the timestamp in ms when the request is sent.

The input to `generator.py` is a workload config file in YAML. The YAML file needs to specify
1. Start time of the experiment. This is when the request traffic starts
2. End time of the experiment.
3. Number of users in the system
4. An array of function names and average request interarrival time (in ms)

The generator then creates request traffics for each function assuming they are Poisson processes.

All function's traffics have the same start and end time.

# How to run experiments

There are 2 ways of running experiments with generated workload files using `snapctr`.
One is using `snapctr --requests_file <workload.json>` and the other is using a TCP load generator.
You can see examples of both in `run_experiment_filegateway.sh` and `run_experiment_tcpgateway.sh`.

You can limit how much memory `snapctr` uses with the `--mem` option.

## Input workload file through `--requests_file` to `snapctr`

`snapctr` has a [FileGateway](https://github.com/princeton-sns/snapfaas/blob/master/src/gateway.rs#L30)
that reads the workload file and sends requests to the worker pool at the correct
time intervals. Requests through the FileGateway are sent directly to the worker pool and 
don't travel through the network.

Note that the FileGateway runs an open-loop experiment where the sending of the next request
does *not* depend on the completion of the previous request.

## Load generator

See `$SNAPFAAS_ROOT/bins/client` for an simple load generator. 

# How to analyze experimental results
