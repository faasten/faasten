"""
This script processes trace data from snapctr.
Each trace file contains data in the format of a JSON string.
The JSON string follows this format:
    {
        "number of requests dropped": an_integer,
        "number of evictions": an_integer,
        "number of requests completed": an_integer,
        "number of vms created": an_integer,
        "boot timestamps": {
            "an_integer_vm_id": [
                start_timestamp,
                end_timestamp
            ],
            "another_integer_vm_id": [
                another_start_timestamp,
                another_end_timestamp
            ],
            ...
        }
        "eviction timestamps": {
            "an_integer_vm_id": [
                start_timestamp,
                end_timestamp
            ],
            "another_integer_vm_id": [
                another_start_timestamp,
                another_end_timestamp
            ],
            ...
        }
        "request/response timestamps": {
            "an_integer_vm_id": [
                start_timestamp,
                end_timestamp
            ],
            "another_integer_vm_id": [
                another_start_timestamp,
                another_end_timestamp
            ],
            ...
        }
        "vm memory sizes": {
            "an_integer_vm_id": an_integer_of_memory_size,
            "another_integer_vm_id": another_integer_of_memory_size,
        }
    }
"""
import json
import yaml
import sys
import numpy as np
import glob
import matplotlib.pyplot as plt
#np.set_printoptions(threshold=sys.maxsize)

SMALLEST_VM = 128
NS2MS = 1000000

def list_to_tuple_list(l):
    if len(l) == 0:
        return l

    if len(l) % 2 == 1:
        sys.exit("list has odd number of elements")

    return [(l[i], l[i+1]) for i in range(0,len(l),2)]

def in_tuple_range(tsp, tuple_range):
    """Return whether a timestamp falls within a time range

    tsp -- timestamp
    tuple_range -- a tuple of (start timestamp, end timestamp)
    """
    return tsp>=tuple_range[0] and tsp<=tuple_range[1]

def overlap(window, time_range):
    """Return the amount of overlap time between 2 time ranges

    0 if all of time_range precedes window
    -1 if all of time_range follows window
    overlap amount otherwise
    """
    if time_range[1] <= window[0]:
        return 0

    if time_range[0] >= window[1]:
        return -1

    l = [time_range[0], time_range[1], window[0], window[1]]
    l.sort()

    return l[2] - l[1]

class VM(object):
    def __init__(self, id, boot_tsp=[], req_rsp_tsp=[], evict_tsp=[], mem=0):
        self.id = id
        self.boot = list_to_tuple_list(boot_tsp)
        self.req_rsp = list_to_tuple_list(req_rsp_tsp)
        self.evict = list_to_tuple_list(evict_tsp)
        self.stage = 0 
        self.req_rsp_idx = 0
        self.mem = mem 

    def __str__(self):
        return "id: " + str(self.id) + " boot: " + str(self.boot) + \
                " req_rsp: " + str(self.req_rsp) + " evict: " + str(self.evict) +\
                " mem size: " + str(self.mem)

    def update(self, boot_tsp=[], req_rsp_tsp=[], evict_tsp=[], mem=0):
        """
        update information in a VM object that's already created
        boot_tsp, evict_tsp and mem should only be set once.
        req_rsp_tsp is appended to VM's req_rsp list
        """
        if self.mem == 0:
            self.mem = mem
        else: # report error if try to set mem more than once
            if mem!= 0:
                sys.exit("try to set mem twice in vm {}".format(self.id))

        if self.boot == []:
            self.boot = list_to_tuple_list(boot_tsp)
        else:
            if boot_tsp!=[]:
                sys.exit("try to set boot_tsp twice in vm {}".format(self.id))

        if self.evict == []:
            self.evict = list_to_tuple_list(evict_tsp)
        else:
            if evict_tsp!=[]:
                sys.exit("try to set evict_tsp twice in vm {}".format(self.id))

        self.req_rsp = self.req_rsp + list_to_tuple_list(req_rsp_tsp)

        return

    def is_running(self, tsp):

        for req_rsp_tuple in self.req_rsp:
            if in_tuple_range(tsp, req_rsp_tuple):
                return self.mem

        return 0

    def boot_time(self):
        """Return the total amount of time that this VM spent in booting"""
        return self.boot[0][1] - self.boot[0][0]

    def runtime(self):
        """Return the total amount of time that this VM spent running app code"""
        runtime = 0
        for r in self.req_rsp:
            runtime = runtime + (r[1] - r[0])

        return runtime

    def evict_time(self):
        """Return the total amount of time that this VM spent in eviction"""
        if self.evict == []:
            return 0

        return self.evict[0][1] - self.evict[0][0]
    
    def idle_time(self):
        """Return the total amount of time that this VM is up but not running app code"""
        return self.uptime() - self.runtime()

    def uptime(self):
        """Return the total amount of time that VM is up"""
        if self.evict == []:
            return end_time - self.boot[0][1]

        return self.evict[0][0] - self.boot[0][1]

    def uptimestamp(self):
        """Return the launch finish timestamp and shutdown start timestamp of this VM in a tuple"""
        if self.evict == []:
            return (self.boot[0][1], end_time)

        return (self.boot[0][1], self.evict[0][0])

    def lifetime(self):
        """Return the total amount of time between start VM command and eviction finishes"""
        if self.evict == []:
            return end_time - self.boot[0][0]

        return self.evict[0][1] - self.boot[0][0]

    def lifetimestamp(self):
        """Return the launch start timestamp and shutdown finish timestamp of this VM in a tuple"""
        if self.evict == []:
            return (self.boot[0][0], end_time)

        return (self.boot[0][0], self.evict[0][1])

    def runtime_in_window(self, window):
        """Return the amount of time within a window that this VM spent running app code

        windown -- a tuple representing a window
        """
        runtime = 0
        for r in self.req_rsp:
            ol = overlap(window, r)
            if ol == -1:
                break
            runtime = runtime + ol

        return runtime


def process_single_trace(data):
    """
    given a JSON object (a dict objet returned by json.load())output from
    snapctr (format specified at the beginning of this file), return
    1. the number of completed requests
    2. the number of dropped requests
    3. the number of evictions
    4. the number of vms created
    5. a list of VM objects.
    as a tuple
    """
    return data['number of requests completed'],\
           data['number of requests dropped'],\
           data['number of evictions'],\
           data['number of vms created'],\
           json_to_vms(data)



def json_to_vms(data):
    """
    given a JSON object (a dict objet returned by json.load()) output from
    snapctr (format specified at the beginning of this file), create and return
    a dict of VM objects with the key being the VM's ID and the value being the
    VM object.

    Specifically, this function processes the following fields from the JSON
    object:
    1. "boot timestamps"
    2. "eviction timestamps"
    3. "request/response timestamps"
    4. "vm memory sizes"
    """
    if data=={}:
        return {}

    vms = {}
    vm_mem_sizes = data['vm memory sizes']
    boot_tsp = data['boot timestamps']
    evict_tsp = data['eviction timestamps']
    req_rsp_tsp = data['request/response timestamps']

    for key in vm_mem_sizes:
        if key in vms:
            sys.exit("key {} is already vm list".format(key))

        vms[key] = VM(key, mem=vm_mem_sizes[key])

    for key in boot_tsp:
        if key in vms:
            vms[key].update(boot_tsp=boot_tsp[key])
        else:
            vms[key] = VM(key, boot_tsp=boot_tsp[key])

    for key in evict_tsp:
        if key in vms:
            vms[key].update(evict_tsp=evict_tsp[key])
        else:
            vms[key] = VM(key, evict_tsp=evict_tsp[key])

    for key in req_rsp_tsp:
        if key in vms:
            vms[key].update(req_rsp_tsp=req_rsp_tsp[key])
        else:
            vms[key] = VM(key, req_rsp_tsp=req_rsp_tsp[key])

    return vms

def merge_vms(v1, v2):
    """
    merge 2 vms objects that have the same ID
    merge means concat their req_rsp, boot and evict.
    make sure that not both of them have non-empty boot and evict.
    Also make sure that if both have non-zero mem, that they are the same value
    """
    if v1.id != v2.id:
        sys.exit("try to merge 2 vms with different ids")

    if v1.boot!=[] and v2.boot!=[]:
        sys.exit("try to merge 2 vms with different boot")

    if v1.evict!=[] and v2.evict!=[]:
        sys.exit("try to merge 2 vms with different evict")
    if v1.mem!=0 and v2.mem!=0 and v1.mem!=v2.mem:
        sys.exit("try to merge 2 vms with different mem")

    v1.boot = v1.boot+v2.boot
    v1.req_rsp = v1.req_rsp+ v2.req_rsp
    v1.evict = v1.evict+v2.evict
    v1.mem=max(v1.mem, v2.mem)
    return v1

def merge_vm_dicts(d1, d2):
    """
    merge two dicts of VM objects. For example if both d1 and d2 has information
    about VM whose ID is i, then merge those information into one VM object.
    Return a single dict of VM objects with d1 and d2's VMs merged.
    """
    for key in d1:
        if key in d2:
            d2[key] = merge_vms(d1[key], d2[key])
        else:
            d2[key] = d1[key]
    return d2

def check_vm(vm):
    return (vm.mem!=0) and vm.boot!=[] and vm.req_rsp!=[]

def validate(vms, stat):
    """
    Make sure the data generated in the vm dict agrees with measured stat data.
    Also, make sure the VM objects are valid (see check_vm for details)
    """
    assert(len(vms) == stat[3])
    num_complete = 0
    num_evict = 0
    for v in list(vms.values()):
        assert(check_vm(v))
        num_complete = num_complete+len(v.req_rsp)
        num_evict = num_evict+len(v.evict)

    assert(num_complete==stat[0])
    assert(num_evict==stat[2])


# stat contains aggregate statistics data in the order of:
#    1. the number of completed requests
#    2. the number of dropped requests
#    3. the number of evictions
#    4. the number of vms created
# The same as the return value of process_single_trace minus the list of VMs at
# the end.
num_workers = len(sys.argv)-1
print("Number of worker threads in this controller: {}".format(num_workers))

stat = np.array([0,0,0,0])
vms = {}
for i in range(1, len(sys.argv)):
    measurement_file = open(sys.argv[i], 'r')
    data = json.load(measurement_file)
    measurement_file.close()
    *s, v = process_single_trace(data)
    s = np.array(s)
    stat, vms = stat + s, merge_vm_dicts(vms, v)

# sort every vm's req_rsp
for v in list(vms.values()):
    v.req_rsp.sort()

# make sure the data makes sense
validate(vms, stat)

vms = list(vms.values())

# print high level statistics
print("***************************")
print("# VMs created: {}".format(stat[3]))
print("# VMs evicted: {}".format(stat[2]))
print("# Requests completed: {}".format(stat[0]))
print("# Requests dropped: {}".format(stat[1]))
print("***************************")
if len(vms) == 0:
    print("Zero vms created. No more analysis left to do. Exiting...")
    exit(0)

# find the experiment start time as the boot start timestamp of the first VM
# find the experiment end time as the complete timestamp of the last request
start_time = vms[0].boot[0][0]
end_time = vms[0].req_rsp[-1][1]

for vm in vms:
    if vm.boot[0][0] < start_time:
        start_time = vm.boot[0][0]
    if vm.req_rsp[-1][1]> end_time:
        end_time = vm.req_rsp[-1][1]

print(start_time)
print(end_time)

# calculate throughput utilization over the timespan of the experiment
# We use a sliding window approach. Throughput is calculated as the number of
# requests completed within a window over the length of the window (in secs)
# to get #req/sec
window_size = 6000*1000000 #ms * 1000000 = ns

throughput = []
wt = start_time + window_size / 2
while wt < end_time:
    window = (wt - window_size / 2, wt + window_size / 2)
    completed = 0;
    for vm in vms:
        for r in vm.req_rsp:
            if r[1] >= window[0] and r[1]<=window[1]:
                completed = completed +1

    throughput.append(completed/(window_size/(1000*1000000))) # #req/sec

    wt = wt + window_size

throughput = np.array(throughput)

# plot
x = np.linspace(0, (end_time/(NS2MS*1000) - start_time/(NS2MS*1000) ), len(throughput)  )

fig = plt.figure()
fig.set_size_inches(8,5)
plt.plot(x, throughput)
plt.xlabel('time(s)')
plt.ylabel('throughput(#req/sec)')
plt.title('Throughput')
plt.savefig('test.png')

plt.show()

#total_idle_time = [vm.idle_time() for vm in vms]
#total_idle_timeMB += vm.idle_time() * vm.mem
#total_boot_time += vm.boot_time()
#total_boot_timeMB += vm.boot_time() * vm.mem
#total_eviction_time += vm.evict_time()
#total_eviction_timeMB += vm.evict_time() * vm.mem
#total_runtime += vm.runtime()
#total_runtimeMB += vm.runtime() * vm.mem
print(stat)
#for v in vms.items():
#    print(v[1])
# average throughput
print("Average throughput:\
        {}#req/sec".format(stat[0]/((end_time-start_time)/(1000000*1000)) ))
sys.exit();





# Old:
start_time = data['start time']/NS2MS
end_time = data['end time']/NS2MS
num_vm = len(data['boot timestamps'])
total_mem = data['total mem']
resource_limit = int(total_mem/SMALLEST_VM) # the maximum number of 128MB VMs that the cluster can support

#function_config_file = open(sys.argv[2], 'r')
#config = yaml.load(function_config_file.read(), Loader=yaml.Loader)
#function_config_file.close()
# get mem size for each function
#function_to_memsize = {}
#for function in config:
#    name = function['name']
#    mem = function['memory']
#    function_to_memsize[name] = mem
#
#print(function_to_memsize)

# scheduler latency
schedule_latency = np.array(data['request schedule latency'])
schedule_latency = schedule_latency / NS2MS

# calculate high-level aggregate metrics
vms = []
all_req_res = []
all_eviction_tsp = []
all_boot_tsp = []
for vm_id in range(3, 3+num_vm,1):
    mem_size = data['vm mem sizes'][str(vm_id)]
    boot_tsp = [l/NS2MS for l in data['boot timestamps'][str(vm_id)]]
    req_res_tsp = [l/NS2MS for l in data['request/response timestamps'][str(vm_id)]]
    all_req_res = all_req_res+req_res_tsp
    all_boot_tsp = all_boot_tsp + boot_tsp

    try:
        evict_tsp = [l/NS2MS for l in data['eviction timestamps'][str(vm_id)]]
        all_eviction_tsp = all_eviction_tsp + evict_tsp
    except:
        evict_tsp = []

    vm = VM(vm_id, boot_tsp, req_res_tsp, evict_tsp, mem_size)
    vms.append(vm)


total_idle_time = 0
total_boot_time = 0
total_eviction_time = 0
total_runtime = 0
total_runtimeMB = 0
total_idle_timeMB = 0
total_eviction_timeMB = 0
total_boot_timeMB = 0


#    print("vm {}, uptime: {}, runtime: {}, idle time: {}, boot time: {}, evict time: {}"\
#            .format(vm.id,\
#                vm.uptime()/1000000,\
#                vm.runtime()/1000000,
#                vm.idle_time()/1000000,\
#                vm.boot_time()/1000000,\
#                vm.evict_time()/1000000))

total_time = total_runtime + total_idle_time + total_boot_time + total_eviction_time
total_experiment_duration = (end_time - start_time)

print("cluster size: {}MB".format(total_mem))
print('cluster can support ' + str(resource_limit) + ' 128MB VMs')
print("booted a total of " + str(num_vm) + " VMs")
print('number of completed requests: {}'.format(data['number of completed requests']))
print('number of dropped requests (resource exhaustion): {}'.format(data['drop requests (resource)']))
print('number of dropped requests (concurrency limit): {}'.format(data['drop requests (concurrency)']))
print('number of evictions: {}'.format(data['number of evictions']))
print('cumulative throughput: {0:.2f}'.format(data['cumulative throughput']))
print("experiment duration: {0:.2f}ms".format(total_experiment_duration))
print("total time (spent by all VMs): {0:.2f}ms".format(total_time))
print("total runtime time: {0:.2f}ms".format(total_runtime))
print("total idle time: {0:.2f}ms".format(total_idle_time))
print("total boot time: {0:.2f}ms".format(total_boot_time))
print("total eviction time: {0:.2f}ms".format(total_eviction_time))
#print("total runtimeMB (ms-MB): {}ms-MB".format(int(total_runtimeMB)))
print("type 1 utilization: {0:.2f}%".format(100*total_runtimeMB/(total_experiment_duration*total_mem)))
print("type 2 utilization: {0:.2f}%".format(100*total_runtimeMB/(total_runtimeMB + total_eviction_timeMB + total_boot_timeMB)))

print("average scheduling latency: {0:.2f}ms".format(np.mean(schedule_latency)))


# calculate utilization over the timespan of the experiment
utilization1 = []
utilization2 = []
runtimemb_all = []
window_size = 6000 #ms

wt = start_time + window_size / 2
while wt < end_time:
    window = (wt - window_size / 2, wt + window_size / 2)
    running = 0
    runtimemb = 0
    for vm in vms:
        runtimemb = runtimemb + vm.runtime_in_window(window) * vm.resource

    utilization1.append(runtimemb/(window_size*total_mem))

    wt = wt + window_size

utilization1 = np.array(utilization1) * 100

# plot
x = np.linspace(0, (end_time - start_time ), len(utilization1)  )

fig = plt.figure()
fig.set_size_inches(8,5)
plt.plot(x, utilization1)
plt.xlabel('time(ms)')
plt.ylabel('Utilization (%)')
plt.title('Utilization')
plt.legend()
plt.savefig('test.png')

plt.show()

