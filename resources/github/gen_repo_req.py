import sys
import json

req = {}
req['function'] = 'github_read_repo'
req['time'] = 0
req['user_id'] = 0
payload = {}
payload['owner'] = sys.argv[1]
payload['repo'] = sys.argv[2]
req['payload'] = payload

print(json.dumps(req))
