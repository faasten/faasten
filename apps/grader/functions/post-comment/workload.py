def handle(req, syscall):
    args = req['args']
    meta = req['meta']
    report = syscall.read(args['report'])
    path = ['github', 'repos', meta['org'], meta['repo'], 'commits', meta['commit'], 'comments']
    syscall.fs_write(path, report)
