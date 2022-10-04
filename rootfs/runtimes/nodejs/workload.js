const vsock = require("vsock");
const { execSync, exec } = require("child_process");

const syscalls = require('./syscalls')

// for snapshot
// this approach relies on that we are currently being executed on cpu 0
// and that other cpus writes to the port before us
// since as of now snapshots are created offline, we are fine
const cpu_count = require("os").cpus().length;
for (var i = 1; i < cpu_count; i++) {
    exec(`taskset -c ${i} outl 124 0x3f0`);
}
execSync('taskset -c 0 outl 124 0x3f0')

execSync("mount -r /dev/vdb /srv");

module.paths.push("/srv/node_modules");
const app = require("/srv/workload");

for (var i = 1; i < cpu_count; i++) {
    exec(`taskset -c ${i} outl 124 0x3f0`);
}
execSync('taskset -c 0 outl 124 0x3f0')

const sock_conn = vsock.connect(2, 1234);
const sc = new syscalls.Syscall(sock_conn);

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

