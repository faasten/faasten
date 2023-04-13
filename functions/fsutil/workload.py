import json
import base64
from time import time

def handle(request, syscall):
    args = request['input']['args']
    op = request['input']['op']
    ret = {}
    if op == 'create-gate':
        path = args['path']
        policy = args['policy']
        app_blob = request['input-blob']
        memory = args['memory']
        runtime = args['runtime']
        ret['starting_time'] = time()
        ret['success'] = syscall.fs_creategate(path, policy, app_blob, memory, runtime)
        ret['finishing_time'] = time()
    elif op == 'create-redirect-gate':
        path = args['path']
        policy = args['policy']
        redirect_path = args['redirect_path']
        ret['starting_time'] = time()
        ret['success'] = syscall.fs_createredirectgate(path, policy, redirect_path)
        ret['finishing_time'] = time()
    elif op == 'create-blob':
        blob = request['input-blob']
        path = args['path']
        label = args['label']
        ret['starting_time'] = time()
        if syscall.fs_linkblob(blob, path, label):
            ret['success'] = True
            #trigger_ret = None
            #if 'triggers' in args:
            #    trigger_ret = trigger(args['triggers'], syscall, op, path)
            #ret['trigger_status'] = trigger_ret
        else:
            ret['success'] = False
        ret['finishing_time'] = time()
    elif op == 'create-file':
        path = args['path']
        label = args['label']
        ret['starting_time'] = time()
        if syscall.fs_createfile(path, label):
            ret['success'] = True
            #trigger_ret = None
            #if 'triggers' in args:
            #    trigger(args['triggers'], syscall, op, path)
            #    trigger_ret = trigger(args['triggers'], syscall, op, path)
            #ret['trigger_status'] = trigger_ret
        else:
            ret['success'] = False
        ret['finishing_time'] = time()
    elif op == 'read-file':
        path = args['path']
        ret['starting_time'] = time()
        v = syscall.fs_read(path)
        if v is None:
            ret['success'] = False
        else:
            # return value type is bytes
            # encode it into utf-8 string
            ret['success'] = True
            encoded = base64.standard_b64encode(v)
            s = encoded.decode()
            ret['value'] = s
        ret['finishing_time'] = time()
    elif op == 'write-file':
        path = args['path']
        data = args['data']
        ret['starting_time'] = time()
        ret['success'] = syscall.fs_write(path, data.encode('utf-8'))
        ret['finishing_time'] = time()
    elif op == 'create-dir':
        path = args['path']
        label = args['label']
        ret['starting_time'] = time()
        ret['success'] = syscall.fs_createdir(path, label)
        ret['finishing_time'] = time()
    elif op == 'list-dir':
        path = args['path']
        ret['starting_time'] = time()
        v = syscall.fs_list(path)
        if v is None:
            ret['success'] = False
        else:
            ret['success'] = True
            ret['value'] = v
        ret['finishing_time'] = time()
    elif op == 'create-faceted':
        path = args['path']
        ret['starting_time'] = time()
        ret['success'] = syscall.fs_createfaceted(path)
        ret['finishing_time'] = time()
    elif op == 'list-faceted':
        path = args['path']
        ret['starting_time'] = time()
        v = syscall.fs_faceted_list(path)
        if v is None:
            ret['success'] = False
        else:
            ret['success'] = True
            ret['value'] = v
    elif op == 'delete':
        path = args['path']
        ret['starting_time'] = time()
        ret['success'] = syscall.fs_delete(path)
        ret['finishing_time'] = time()
    elif op == 'create-service':
        path    = args['path']
        policy  = args['policy']
        label   = args['label']
        url     = args['url']
        verb    = args['verb']
        headers = args['headers']
        ret['starting_time'] = time()
        ret['success'] = syscall.fs_createservice(path, policy, label, url, verb, headers)
        ret['finishing_time'] = time()
    elif op == 'declassify':
        # TODO only for microbenchmark!
        secrecy = args['secrecy']
        parsed = syscall.buckle_parse(secrecy + ',T')
        ret['starting_time'] = time()
        syscall.declassify(parsed.secrecy)
        ret['finishing_time'] = time()
    elif op == 'endorse':
        # TODO only for microbenchmark!
        ret['starting_time'] = time()
        syscall.endorse()
        ret['finishing_time'] = time()
    elif op == 'taint':
        # TODO only for microbenchmark!
        label = args['label']
        label = syscall.buckle_parse(label)
        ret['starting_time'] = time()
        syscall.taint_with_label(label)
        ret['finishing_time'] = time()
    elif op == 'gen-blob':
        # TODO only for microbenchmark!
        data = b''
        ret['starting_time'] = time()
        with syscall.create_blob() as newblob:
            newblob.finalize(data)
        ret['finishing_time'] = time()
    else:
        ret['success'] = False
        ret['error'] = '[fsutil] unknown op'
    return ret

#def trigger(triggers, syscall, op, path):
#    ret = {'success': [], 'failure': []}
#    for gate in triggers:
#        payload = {
#            'source-op': op,
#            'object-path': path,
#        }
#        if syscall.invoke(gate, json.dumps(payload)):
#            ret['success'].append(gate)
#        else:
#            ret['failure'].append(gate)
#    return ret
