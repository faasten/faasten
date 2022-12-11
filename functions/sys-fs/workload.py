import json

def handle(req, syscall):
    op = req['args']['op']
    user_facet = syscall.buckle_parse('user,user')
    if op == 'lsroot':
        success = syscall.fs_list([]) is not None
    elif op == 'createuserfile':
        success = syscall.fs_createfile(['home', user_facet, 'file1'], label=user_facet)
    elif op == 'writeuserfile':
        data = bytes(req['args']['data'].encode('utf-8'))
        success = syscall.fs_write(['home', user_facet, 'file1'], data)
    elif op == 'readuserfile':
        success = syscall.fs_read(['home', user_facet, 'file1']) is not None
    elif op == 'deleteuserfile':
        success = syscall.fs_delete(['home', user_facet, 'file1'])
    else:
        return {'error': 'unsupported op.'}
    return {'success': success}
