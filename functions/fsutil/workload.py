import json
import base64

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
        ret['success'] = syscall.fs_creategate(path, policy, app_blob, memory, runtime)
    elif op == 'create-redirect-gate':
        path = args['path']
        policy = args['policy']
        redirect_path = args['redirect_path']
        ret['success'] = syscall.fs_createredirectgate(path, policy, redirect_path)
    elif op == 'create-blob':
        blob = request['input-blob']
        path = args['path']
        label = None
        if 'label' in args:
            label = args['label']
        if syscall.fs_linkblob(blob, path, label):
            ret['success'] = True
            #trigger_ret = None
            #if 'triggers' in args:
            #    trigger_ret = trigger(args['triggers'], syscall, op, path)
            #ret['trigger_status'] = trigger_ret
        else:
            ret['success'] = False
    elif op == 'create-file':
        path = args['path']
        label = args['label']
        if syscall.fs_createfile(path, label):
            ret['success'] = True
            #trigger_ret = None
            #if 'triggers' in args:
            #    trigger(args['triggers'], syscall, op, path)
            #    trigger_ret = trigger(args['triggers'], syscall, op, path)
            #ret['trigger_status'] = trigger_ret
        else:
            ret['success'] = False
    elif op == 'read-file':
        path = args['path']
        v = syscall.fs_read(path)
        if v is None:
            ret['success'] = False
        else:
            # return value type is bytes
            # encode it into utf-8 string
            ret['success'] = True
            encoded = base64.b64encode(v)
            s = encoded.decode()
            ret['value'] = s
    elif op == 'write-file':
        path = args['path']
        data = args['data']
        ret['success'] = syscall.fs_write(path, data)
    elif op == 'create-dir':
        path = args['path']
        label = args['label']
        ret['success'] = syscall.fs_createdir(path, label)
    elif op == 'list-dir':
        path = args['path']
        v = syscall.fs_list(path)
        if v is None:
            ret['success'] = False
        else:
            ret['success'] = True
            ret['value'] = v
    elif op == 'create-faceted':
        path = args['path']
        ret['success'] = syscall.fs_createfaceted(path)
    elif op == 'list-faceted':
        path = args['path']
        v = syscall.fs_faceted_list(path)
        if v is None:
            ret['success'] = False
        else:
            ret['success'] = True
            ret['value'] = v
    elif op == 'delete':
        path = args['path']
        ret['success'] = syscall.fs_delete(path)
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
