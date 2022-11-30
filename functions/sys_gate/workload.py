import json

def handle(req, syscall):
    op = req["op"]
    args = req["args"]
    gate = args["gate"]
    if op == "dup_gate":
        policy = syscall.buckle_parse(args["policy"])
        return json.loads({"success": syscall.dup_gate(gate, policy)})
    elif op == "invoke_gate":
        payload = args["payload"]
        return json.loads({"success": syscall.invoke_gate(gate, payload)})
    else:
        return json.loads({"error": "Invalid op"})
