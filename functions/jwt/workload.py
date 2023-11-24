from syscalls import ResponseStr
import json

from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives import serialization
import jwt
import datetime

PEM_FILE=['home', 'faasten,faasten', 'private_key']
def handle(syscall, payload=b'', blobs={}, invoker=[], **kwargs):
    request = json.loads(payload)
    sub = request['sub']
    # the invoker should be of length 1 and the tokens list should also be of length 1
    idp = invoker[0].tokens[0]
    # read private key PEM file
    with syscall.root().open_at(PEM_FILE) as f:
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

    # TODO: declassify to remove "faasten" in secrecy
    # syscall.declassify(f"{idp}")

    # TODO:register fsutil for first timers
    # i.e. copy the public fsutil gate over to ["home", f"{idp}/{sub},{idp}/{sub}", "fsutil"]
    # Note that this gate should run with the privilege {idp}&faasten

    return ResponseStr(encoded_jwt)
