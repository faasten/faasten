from syscalls import ResponseRaw
BASE_URL = "sns40.cs.princeton.edu"
def handle(syscall, payload=b'', blobs={}, **kwargs):
    redirect_url = "https://fed.princeton.edu/cas"
    # TODO replace the URL to the path to the auth gate
    callback_url = f"{BASE_URL}/authenticate/cas"
    body = f"{redirect_url}/login?service={callback_url}"
    status_code = 302
    return ResponseRaw(body.encode('utf-8'), status_code)
