import json
import base64

def handle(request, cloudcall):
    args = request['input']['args']
    op = request['input']['op']
    # the fsutil is private, therefore, any public or less secret output requires declassification
    if 'declassify-to' in request['input']:
        target = cloudcall.buckle_parse(request['input']['declassify-to']+',T')
        if target:
            if cloudcall.declassify(target.secrecy) is None:
                ret['declassify-to'] = 'failed declassification'
        else:
            ret['declassify-to'] = 'invalid secrecy string'
    ret = {}
    if op == 'create-gate':
        path = args['path']
        policy = args['policy']
        app_blob = request['input-blob']
        memory = args['memory']
        runtime = args['runtime']
        ret['success'] = cloudcall.fs_creategate(path, policy, app_blob, memory, runtime)
    elif op == 'create-redirect-gate':
        path = args['path']
        policy = args['policy']
        redirect_path = args['redirect_path']
        ret['success'] = cloudcall.fs_createredirectgate(path, policy, redirect_path)
    elif op == 'dup-gate':
        src = args['src']
        dest = args['dest']
        policy = args['policy']
        ret['success'] = cloudcall.fs_dupgate(src, dest, policy)
    elif op == 'create-blob':
        blob = request['input-blob']
        path = args['path']
        label = None
        if 'label' in args:
            label = cloudcall.buckle_parse(args['label'])
        if cloudcall.fs_linkblob(blob, path, label):
            ret['success'] = True
            #trigger_ret = None
            #if 'triggers' in args:
            #    trigger_ret = trigger(args['triggers'], cloudcall, op, path)
            #ret['trigger_status'] = trigger_ret
        else:
            ret['success'] = False
    elif op == 'create-file':
        path = args['path']
        label = args['label']
        if cloudcall.fs_createfile(path, label):
            ret['success'] = True
            #trigger_ret = None
            #if 'triggers' in args:
            #    trigger(args['triggers'], cloudcall, op, path)
            #    trigger_ret = trigger(args['triggers'], cloudcall, op, path)
            #ret['trigger_status'] = trigger_ret
        else:
            ret['success'] = False
    elif op == 'read-file':
        path = args['path']
        v = cloudcall.fs_read(path)
        if v is None:
            ret['success'] = False
        else:
            # return value type is bytes
            # encode it into utf-8 string
            ret['success'] = True
            encoded = base64.standard_b64encode(v)
            s = encoded.decode()
            ret['value'] = s
    elif op == 'write-file':
        path = args['path']
        data = args['data']
        ret['success'] = cloudcall.fs_write(path, data.encode('utf-8'))
    elif op == 'create-dir':
        path = args['path']
        label = args['label']
        ret['success'] = cloudcall.fs_createdir(path, label)
    elif op == 'list-dir':
        path = args['path']
        v = cloudcall.fs_list(path)
        if v is None:
            ret['success'] = False
        else:
            ret['success'] = True
            ret['value'] = v
    elif op == 'create-faceted':
        path = args['path']
        ret['success'] = cloudcall.fs_createfaceted(path)
    elif op == 'list-faceted':
        path = args['path']
        v = cloudcall.fs_faceted_list(path)
        if v is None:
            ret['success'] = False
        else:
            ret['success'] = True
            ret['value'] = v
    elif op == 'delete':
        path = args['path']
        ret['success'] = cloudcall.fs_delete(path)
    elif op == 'create-service':
        path    = args['path']
        policy  = args['policy']
        label   = args['label']
        url     = args['url']
        verb    = args['verb']
        headers = args['headers']
        ret['success'] = cloudcall.fs_createservice(path, policy, label, url, verb, headers)
    else:
        ret['success'] = False
        ret['error'] = '[fsutil] unknown op'
    return ret

#def trigger(triggers, cloudcall, op, path):
#    ret = {'success': [], 'failure': []}
#    for gate in triggers:
#        payload = {
#            'source-op': op,
#            'object-path': path,
#        }
#        if cloudcall.invoke(gate, json.dumps(payload)):
#            ret['success'].append(gate)
#        else:
#            ret['failure'].append(gate)
#    return ret
