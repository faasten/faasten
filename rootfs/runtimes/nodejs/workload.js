const vsock = require("vsock");
const syscalls = require('./syscalls')

// load app
const app = require("/srv/workload");

const sock = vsock.connect(2, 1234);
const sc = new syscalls.Syscall(sock);

(async function() {
    while (true) {
        // pre request
        const req = await sc.request();
        const hrstart = process.hrtime();

        // handle request
        const resp = await app.handle(req, sc);

        // post request
        const hrend = process.hrtime(hrstart)
        resp.runtime_sec = hrend[0];
        resp.runtime_ms = hrend[1] / 1000000;

        await sc.respond(resp);
        // TODO error handling
    }
})();

