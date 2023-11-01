from syscalls import ResponseStr
import json

from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives import serialization
import jwt
import datetime

PEM_FILE=['home', 'faasten,faasten', 'signkey']
def handle(syscall, payload=b'', blobs={}, env={}, **kwargs):
    request = json.loads(payload)
    sub = request['sub']
    idp = 'princeton.edu' # TODO read this from env
    # read private key PEM file
    with syscall.root().open_at(PEM_FILE) as entry:
        with entry.get() as f:
            data = f.read()
            private_key = serialization.load_pem_private_key(
                data,
                password=None,  # replace with your password if the private key is encrypted
                backend=default_backend()
            )
    claims = {
        "sub": f"{idp}/{sub}",          # subject (typically a user id)
        "iat": datetime.datetime.utcnow(),             # issued at
        "exp": datetime.datetime.utcnow() + datetime.timedelta(days=1),  # expiration time
    }
    encoded_jwt = jwt.encode(claims, private_key, algorithm='ES256')
    return ResponseStr(encoded_jwt)
