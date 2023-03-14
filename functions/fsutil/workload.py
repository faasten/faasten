import json

def handle(req, syscall):
    args = req['args']
    op = args['op']
    if op == 'create-gate':
        if 
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
    return {'success': success}
