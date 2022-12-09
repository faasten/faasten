import numpy as np
from enum import Enum
import yaml
import operator
import functools
import random
import sys
import json

np.set_printoptions(threshold=sys.maxsize)

def finished(si, workload):
    for i,s in enumerate(si):
        if s < len(workload[i]) - 1:
            return False
    return True

## Generate request timestamps within a windown ([start,end]) from exponential
#  distribution with mean = mu
#
# Given a start timestamp and an end timestamp, generate the timestamps of requests
# whose inter-arrival time follows an exponential distribution/
# All timestamps will fall within [start, end] range.
#
# @param start Beginning of the window. All generated timestamps will be greater than
#              or equal to this value. This is not the timestamp of the first request
#              (although it's possible that it might be).
# @param end End of the window. All generated timestamps will be smaller than or equal
#            to this value.
# @param mu Mean of the exponential distribution. This represents the average inter-arrival
#           time in ms
def generate_request_timestamps(start, end, mu):
    duration = end - start
    expected_num_requests = int(duration/mu)

    inter_arrival_time = np.random.exponential(int(mu), (expected_num_requests, 1))
    inter_arrival_time = np.ceil(inter_arrival_time)
    inter_arrival_time_cumsum = np.cumsum(inter_arrival_time, axis=0)

    # shift all timestamps by `start` so that they fall within the window
    inter_arrival_time_cumsum = inter_arrival_time_cumsum + start
    inter_arrival_time_cumsum = inter_arrival_time_cumsum.astype(int)
    timestamp = inter_arrival_time_cumsum[inter_arrival_time_cumsum <= end]

    return timestamp


# @param functions a list of function configuration objects
def alternate_generator(functions, start, end, num_users):
    elm_type = [('timestamp', int), ('user_id', int), ('function_name', 'U100')]
    workload = np.array([], dtype=elm_type)
    toolbar_width=40
    sys.stderr.write("[%s]" % (" " * toolbar_width))
    sys.stderr.flush()
    sys.stderr.write("\b" * (toolbar_width+1)) # return to start of line, after '['
    one_hundredth = (len(functions) * num_users) / toolbar_width
    i = 0
    for function in functions:
        for user_id in range(0, num_users):
            arrivals = generate_request_timestamps(start, end, function['mu'])
            vfunc = np.frompyfunc(lambda x: (x, user_id, function['name']), 1, 1)
            workload = np.append(workload, vfunc(arrivals).astype(elm_type))
            i += 1
            if i % one_hundredth == 0:
                sys.stderr.write('*')
                sys.stderr.flush()
            #print('User %d' % user_id, file=sys.stderr)
    return np.sort(workload, order=['timestamp'])


def find_function_index_and_user_id(num_user_cumsum, index):

    for i, cumsum in enumerate(num_user_cumsum):
        if index < cumsum:
            break
    if i == 0:
        return i, index
    else:
        return i, index - num_user_cumsum[i-1]


if __name__ == "__main__":
    # during non-spike periods, we assume each function has a steady stream
    # of requests coming in at 1 request per second or 0.001 request per ms
    default_arrival_rate = 0.001 
    default_mu = 1 / default_arrival_rate

    workload_config_file = sys.argv[1]
    print('loading workload config from: ' + workload_config_file, file=sys.stderr)

    with open(workload_config_file) as f:
        config = f.read()

    data = yaml.load(config, Loader=yaml.Loader) # a list of dicts
    workload = alternate_generator(data['functions'], data['start_time'], data['end_time'], data['num_users'])
    for request in workload[['timestamp', 'user_id', 'function_name']]:
        print("%s" % json.dumps({
            'time': int(request[0]),
            'user_id': int(request[1]),
            'function': request[2],
            'payload': {'request': 42}
        }))
    exit()

    output_request_file = sys.argv[2]

    function_names = [f['name'] for f in data]
    mus = np.array([f['mu'] for f in data]) # average inter-arrival time in ms
    start_times = np.array([f['start_time'] for f in data])
    end_times= np.array([f['end_time'] for f in data])
    num_users = np.array([f['users'] for f in data])
    users_cumsum = np.cumsum(np.array(num_users, dtype=np.int32))
    arrival_rates = 1/mus # num of invocations per ms

    print(num_users)
    print(users_cumsum)

    num_functions = len(arrival_rates)

    print('function names: ' + str(function_names))
    print('arrival rates: ' + str(arrival_rates) + "req/ms")
    print("mu:" +str(mus)+"ms")
    print('start time: ' + str(start_times)+"ms")
    print('end time: ' + str(end_times)+"ms")

    # Generate inter-arrival time for all functions
    max_end = end_times.max()
    # each element is a np.array() of timestamps for a particular function. Using
    # list instead of np.array() allows different sizes for each np.array
    workload = [] 

    for spike_start, spike_end, spike_mu, users in zip(start_times, end_times, mus, num_users):
        # during non-spike period, we assume that the function will have the
        # default_arrival_rate defined at the beginning. So for a function with
        # spike_start = 5000 and spike_end = 10000, we also need to generate
        # timestamps for [0, 5000] and (possibly) [10000, max_end]
        windows = [spike_start, spike_end]
        if spike_start > 0:
            windows.insert(0,0)

        if spike_end < max_end:
            windows.append(max_end)

        for u in range(users):
            timestamp = []
            for i in range(len(windows)-1):
                mu = default_mu
                if windows[i] == spike_start:
                    mu = spike_mu

                timestamp = np.append(timestamp, \
                                      generate_request_timestamps(windows[i], windows[i+1], mu))

            workload.append(timestamp)



    search_index = np.zeros(len(workload), dtype=np.int32)

    #inter_arrival_time_cumsum = inter_arrival_time_cumsum.astype(int)

    fd = open(output_request_file, 'w')

    pmin = 0

    print("total number of functions: {}".format(len(workload)))
    while not finished(search_index, workload):
        candidates = [ workload[i][search_index[i]] for i in range(len(workload))]
        minv = np.min(candidates)
        min_idx = np.argmin(candidates)

        interval = minv - pmin
        pmin = minv

        function_name_idx, user_id = find_function_index_and_user_id(users_cumsum, min_idx)

        json.dump({"time": int(interval), "function": function_names[function_name_idx],\
                "user_id": int(user_id), "payload":{"request": 42}}, fd)
        fd.write('\n')

        search_index[min_idx] = search_index[min_idx] + 1

        if search_index[min_idx] == len(workload[min_idx]):
            search_index[min_idx] = search_index[min_idx] - 1
            workload[min_idx][search_index[min_idx]] = sys.maxsize


    fd.close()
