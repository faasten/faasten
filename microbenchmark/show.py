import os, json, numpy

#  common = [
    #  'get',
    #  'put',
    #  'add',
    #  'cas',
#  ]

micros = {
    'create-dir'       : 'create_dir'     ,
    'create-faceted'   : 'create_faceted' ,
    'create-file'      : 'create_file'    ,
    'delete-dir'       : 'delete'         ,
    'delete-faceted'   : 'delete'         ,
    'delete-file'      : 'delete'         ,
    'gen-blob'         : 'gen_blob'       ,
    'label-declassify' : 'declassify'     ,
    'label-endorse'    : 'endorse'        ,
    'label-taint'      : 'taint'          ,
    'read-dir'         : 'list_dir'       ,
    #  'read-faceted'     : 'list_faceted'   ,
    'read-file'        : 'read'           ,
    'write'            : 'write'          ,
}

CURDIR   = os.getcwd()
OUTDIR   = os.path.join(CURDIR, 'output')
STATSDIR = os.path.join(CURDIR, 'stats')

assert os.path.isdir(OUTDIR)
assert os.path.isdir(STATSDIR)

def is_json_str(x):
    try:
        json.loads(x)
    except:
        return False
    return True

def is_ts(x):
    return isinstance(x, dict) and 'secs' in x and 'nanos' in x

def process_stats(jsons):
    return [
        [ { k: ts(v) if is_ts(v) else v for k,v in j.items() } for j in js ]
        for js in jsons
    ]

def process_outputs(jsons):
    return [
        [ float(j['finishing_time']) - float(j['starting_time']) for j in js ]
        for js in jsons
    ]

def get_jsons(base):
    files = [ os.path.join(base, n) for n in os.listdir(base) ]
    return [
        list(map(lambda l: json.loads(l), filter(lambda l: is_json_str(l), open(f).readlines())))
        for f in files
    ]

def get_outputs(micro):
    base = os.path.join(OUTDIR, micro)
    return get_jsons(base)

def get_stats(micro):
    base = os.path.join(STATSDIR, micro)
    return get_jsons(base)


def ts(obj):
    secs = int(obj['secs'])
    nanos = int(obj['nanos'])
    return numpy.datetime64(secs * 1000000000 + nanos, 'ns')


def calc_store_latencies(old, new):
    return new['add'] - old['add'] + new['put'] - old['put'] + new['get'] - old['get'] + new['cas'] - old['cas']

def calc_serde_latencies(old, new):
    return new['ser_dir'] - old['ser_dir'] + new['ser_faceted'] - old['ser_faceted'] + new['ser_label'] - old['ser_label'] + new['de_dir'] - old['de_dir'] + new['de_faceted'] - old['de_faceted']

def calc_label_latencies(old, new):
    return new['label_tracking'] - old['label_tracking']

def calc_op_latencies(old, new, op):
    return new[op] - old[op]

def avg(l):
    return sum(l) / len(l)


if __name__ == "__main__":
    for m, n in micros.items():
        print(m)
        outputs = process_outputs(get_outputs(m))
        outputs = [ o[0] for o in outputs ]
        execution_time = avg(outputs)
        print(execution_time)

        stats = process_stats(get_stats(m))
        stats_store = [ int(calc_store_latencies(s[0], s[1])) for s in stats ]
        stats_serde = [ int(calc_serde_latencies(s[0], s[1])) for s in stats ]
        stats_label = [ int(calc_label_latencies(s[0], s[1])) for s in stats ]
        stats_op    = [ int(calc_op_latencies(s[0], s[1], n)) for s in stats ]

        print(avg(stats_store) / 1000000000)
        print(avg(stats_serde) / 1000000000)
        print(avg(stats_label) / 1000000000)
        print(avg(stats_op) / 1000000000)
        print()

        #  print(stats[1][0]["get"])
        #  break

