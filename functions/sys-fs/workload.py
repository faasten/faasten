import time

def handle(req, syscall):
    response = {'latencies': {}, 'success': {}}
    latencies = response['latencies']
    success = response['success']

    start = time.monotonic_ns()
    user_facet = syscall.buckle_parse('user,user')
    latencies['buckle_parse'] = time.monotonic_ns() - start

    start = time.monotonic_ns()
    label = syscall.get_current_label()
    latencies['get_current_label'] = time.monotonic_ns() - start

    start = time.monotonic_ns()
    delegated = syscall.sub_privilege(['myapp','myfunc'])
    latencies['sub_privilege'] = time.monotonic_ns() - start
 
    start = time.monotonic_ns()
    success['list'] = syscall.fs_list([]) is not None
    latencies['list'] = time.monotonic_ns() - start

    start = time.monotonic_ns()
    success['faceted_list'] = syscall.fs_faceted_list(['home']) is not None
    latencies['faceted_list'] = time.monotonic_ns() - start

    start = time.monotonic_ns()
    success['create'] = syscall.fs_createfile(['home', user_facet, 'file1'], label=user_facet)
    latencies['create'] = time.monotonic_ns() - start

    data = bytes(req['args']['data'].encode('utf-8'))
    start = time.monotonic_ns()
    success['write'] = syscall.fs_write(['home', user_facet, 'file1'], data)
    latencies['write'] = time.monotonic_ns() - start

    start = time.monotonic_ns()
    success['read'] = syscall.fs_read(['home', user_facet, 'file1']) is not None
    latencies['read'] = time.monotonic_ns() - start
 
    start = time.monotonic_ns()
    success['delete'] = syscall.fs_delete(['home', user_facet, 'file1'])
    latencies['delete'] = time.monotonic_ns() - start

    start = time.monotonic_ns()
    label = syscall.taint_with_label(user_facet)
    latencies['taint_with_label'] = time.monotonic_ns() - start

    start = time.monotonic_ns()
    label = syscall.endorse(delegated)
    latencies['endorse'] = time.monotonic_ns() - start

    start = time.monotonic_ns()
    label = syscall.declassify(label.secrecy)
    latencies['declassify'] = time.monotonic_ns() - start

    return response
