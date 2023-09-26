import time
import json
import base64
from contextlib import ExitStack

def handle(syscall, payload=b'', blobs={}, **kwargs):
    request = json.loads(payload)
    args = request['args']
    op = request['op']
    ret = {}
    match op:
        case "ping":
            ret["success"] = True
            ret["value"] = "pong"
        case "mkdir":
            ret["success"] = False
            print("1")
            with syscall.root().open_at(args["base"]) as dir:
                print("2")
                label = syscall.buckle_parse(args["label"])
                res = syscall.dent_create_dir(label)
                print("3")
                if res is not None:
                    print("4")
                    newfd = res.fd
                    res2 = syscall.link(dir.fd, res.fd, args["name"])
                    if res2 is not None:
                        print("5")
                        ret["success"] = res2.success
                        ret["value"] = res.fd
        case "ls":
            ret["success"] = False
            print(args["path"])
            with syscall.root().open_at(args["path"]) as dir:
                res = dir.ls()
                if res is not None:
                    ret["success"] = True
                    ret["value"] = res
        case "unlink":
            ret["success"] = False
            with syscall.root().open_at(args["base"]) as dir:
                res = dir.unlink(args["name"])
                if res is not None:
                    ret["success"] = res.success
                    ret["value"] = res.fd
        case "mkfile":
            ret["success"] = False
            with syscall.root().open_at(args["base"]) as dir:
                label = syscall.buckle_parse(args["label"])
                res = syscall.dent_create_file(label)
                if res is not None:
                    newfd = res.fd
                    res2 = syscall.link(dir.fd, newfd, args["name"])
                    if res2 is not None:
                        ret["success"] = res2.success
                        ret["value"] = res.fd
        case "write":
            ret["success"] = False
            with syscall.root().open_at(args["path"]) as file:
                res = file.write(base64.b64decode(args["data"]))
                ret["success"] = res
        case "read":
            ret["success"] = False
            with syscall.root().open_at(args["path"]) as file:
                res = file.read()
                if res is not None:
                    ret["success"] = True
                    ret["value"] = base64.b64encode(res).decode()
                else:
                    ret["success"] = False
        case "mkgate":
            with ExitStack() as stack:
                dir = stack.enter_context(syscall.root().open_at(args["base"]))
                label = syscall.buckle_parse(args["label"])
                priv = syscall.buckle_parse(args["privilege"] + ",T").secrecy
                clearance = syscall.buckle_parse(args["clearance"] + ",T").secrecy
                res = None
                print(label, blobs)
                if "memory" in args:
                    memory = args["memory"]
                    app_image = None
                    runtime = None
                    kernel = None
                    if blobs.get("app_image"):
                        app_image = syscall.dent_create_blob(label, blobs["app_image"])
                    else:
                        app_image = stack.enter_context(syscall.root().open_at(args["app_image"]))

                    if blobs.get("runtime"):
                        runtime = syscall.dent_create_blob(label, blobs["runtime"])
                    else:
                        runtime = stack.enter_context(syscall.root().open_at(args["runtime"]))

                    if blobs.get("kernel"):
                        kernel = syscall.dent_create_blob(label, blobs["kernel"])
                    else:
                        kernel = stack.enter_context(syscall.root().open_at(args["kernel"]))

                    res = syscall.dent_create_direct_gate(
                        label,
                        priv,
                        clearance,
                        memory,
                        app_image,
                        runtime,
                        kernel)
                else:
                    gate = stack.enter_context(syscall.root().open_at(args["gate"]))
                    res = syscall.dent_create_redirect_gate(
                        label,
                        priv,
                        clearance,
                        gate)
                if res is not None:
                    newfd = res.fd
                    print(res)
                    res2 = dir.link(res, args["name"])
                    if res2 is not None:
                        ret["success"] = res2.success
                        ret["value"] = res.fd
        case "upgate":
            ret["success"] = False
            label = syscall.buckle_parse("T,T")
            priv = args.get("privilege") and syscall.buckle_parse(args.get("privilege") + ",T").secrecy
            clearance = args.get("clearance") and syscall.buckle_parse(args.get("clearance") + ",T").secrecy
            memory = args.get("memory")
            gate = args.get("gate")
            with ExitStack() as stack:
                target_gate = stack.enter_context(syscall.root().open_at(args["path"]))
                if memory:
                    app_image = None
                    runtime = None
                    kernel = None
                    if blobs.get("app_image"):
                        app_image = syscall.dent_create_blob(label, blobs["app_image"])
                    elif args.get("app_image"):
                        app_image = stack.enter_context(syscall.root().open_at(args["app_image"]))

                    if blobs.get("runtime"):
                        runtime = syscall.dent_create_blob(label, blobs["runtime"])
                    elif args.get("runtime"):
                        runtime = stack.enter_context(syscall.root().open_at(args["runtime"]))

                    if blobs.get("kernel"):
                        kernel = syscall.dent_create_blob(label, blobs["kernel"])
                    elif args.get("kernel"):
                        kernel = stack.enter_context(syscall.root().open_at(args["kernel"]))

                    res = target_gate.update_direct(privilege = priv, invoker_clearance = clearance, memory = memory, app_image=app_image, kernel=kernel, runtime=runtime)
                    ret["success"] = res.success
                elif gate:
                    gate = stack.enter_context(syscall.root().open_at(args["gate"]))
                    res = target_gate.update_redirect(privilege = priv, invoker_clearance = clearance, gate = gate)
                    ret["success"] = res.success
                else:
                    ret["success"] = False
                    ret["error"] = "You must pass supply either `memory` or `gate`"
        case "mkblob":
            ret["success"] = False
            base = args["base"]
            label = args["label"]
            label = syscall.buckle_parse(args["label"])
            ret["fds"] = []
            with syscall.root().open_at(base) as dir:
                for (name, blob) in blobs.items():
                    res = syscall.dent_create_blob(label, blob)
                    if res is not None:
                        newfd = res.fd
                        res2 = syscall.link(dir.fd, newfd, name)
                        if res2 is not None:
                            ret["success"] = res2.success
                            list.append(ret["fds"], res.fd)
        case "cat":
            with syscall.root().open_at(args["path"]) as blobfd:
                with blobfd.get() as blob:
                    contents = blob.read()
                    return contents
        case "mkfaceted":
            ret["success"] = False
            with syscall.root().open_at(args["base"]) as dir:
                ret["value"] = args["base"]
                res = syscall.dent_create_faceted()
                if res is not None:
                    newfd = res.fd
                    res2 = syscall.link(dir.fd, newfd, args["name"])
                    if res2 is not None:
                        ret["success"] = res2.success
                        ret["value"] = res.fd
        case 'mksvc':
            label = syscall.buckle_parse(args["label"])
            priv = syscall.buckle_parse(args["privilege"] + ",T").secrecy
            clearance = syscall.buckle_parse(args["clearance"] + ",T").secrecy
            taint = syscall.buckle_parse(args["taint"] + ",T")
            url     = args['url']
            verb    = args['verb']
            headers = args['headers']

            ret["success"] = False
            with syscall.root().open_at(args["base"]) as dir:
                ret["value"] = args["base"]
                res = syscall.dent_create_service(label, priv, clearance, taint, url, verb, headers)
                if res is not None:
                    newfd = res.fd
                    res2 = syscall.link(dir.fd, newfd, args["name"])
                    if res2 is not None:
                        ret["success"] = res2.success
                        ret["value"] = res.fd
        case 'invoke':
            path = args["path"]
            sync = args["sync"]
            payload = base64.b64decode(args["payload"])
            params = args["params"]
            with syscall.root().open_at(args["path"]) as invokable:
                result = invokable.invoke(payload=payload, sync=sync, params=params)
                if result:
                    ret["success"] = result.success
                    ret["fd"] = result.fd
                    ret["data"] = base64.b64encode(result.data).decode("utf-8")
                else:
                    ret["success"] = False
        case _:
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
