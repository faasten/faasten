import json

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
    elif op == 'create-file':
        path = args['path']
        label = args['label']
        ret['success'] = syscall.fs_createfile(path, label)
    elif op == 'read-file':
        path = args['path']
        v = syscall.fs_read(path)
        if v is None:
            ret['success'] = False
        else:
            ret['success'] = True
            ret['value'] = v
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
    #if op == 'createdir':
    #    success = syscall.fs_createdir(args['path']) is not None
    #elif op == 'createfile':
    #    success = syscall.fs_createfile(['home', user_facet, 'file1'], label=user_facet)
    #elif op == 'write':
    #    data = bytes(req['args']['data'].encode('utf-8'))
    #    success = syscall.fs_write(['home', user_facet, 'file1'], data)
    #elif op == 'read':
    #    success = syscall.fs_read(['home', user_facet, 'file1']) is not None
    #elif op == 'list':
    #    success = syscall.fs_list(
    #elif op == 'deleteuserfile':
    #    success = syscall.fs_delete(['home', user_facet, 'file1'])
    #else:
    #    return {'error': 'unsupported op.'}
    return ret
