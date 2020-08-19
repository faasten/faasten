import process_data
import sys
import os
import matplotlib.pyplot as plt

def process_experiments(experiments_dir):
    print("processing experiments under {}".format(experiments_dir))

    experiments = [d for d in os.listdir(experiments_dir) if os.path.isdir(os.path.join(experiments_dir,d))]
    print(experiments)

    results = [process_data.process_experiment(os.path.join(experiments_dir, d)) for d in experiments]
    results = sorted(results, key = lambda i: i[0]['cluster memory']) 

    return results

def success_rate(stat):
    return stat['num requests completed'] / (stat['num requests completed']+stat['num requests dropped'])*100

def plot_success_rate(results):
    memories = [d[0]['cluster memory']/1024 for d in results]
    success_rates = [success_rate(d[0]) for d in results]

    fig = plt.figure()
    fig.set_size_inches(8,5)
    plt.plot(memories, success_rates)
    plt.xlabel('Memory (GB)')
    plt.ylabel('Servicable requests (%)')
    plt.legend()
    plt.savefig("success_rate.png")

def plot_vms_created(results):
    memories = [d[0]['cluster memory']/1024 for d in results]
    vms_created= [d[0]['num vms created'] for d in results]

    fig = plt.figure()
    fig.set_size_inches(8,5)
    plt.plot(memories, vms_created)
    plt.xlabel('Memory (GB)')
    plt.ylabel('Number of VMs created')
    plt.legend()
    plt.savefig("num_vms_created.png")

def plot_vms_evicted(results):
    memories = [d[0]['cluster memory']/1024 for d in results]
    vms_evicted= [d[0]['num vms evicted'] for d in results]

    fig = plt.figure()
    fig.set_size_inches(8,5)
    plt.plot(memories, vms_evicted)
    plt.xlabel('Memory (GB)')
    plt.ylabel('Number of VMs evicted')
    plt.legend()
    plt.savefig("num_vms_evicted.png")



if __name__ == '__main__':
    results = process_experiments(sys.argv[1])

    plot_success_rate(results)
    plot_vms_created(results)
    plot_vms_evicted(results)

