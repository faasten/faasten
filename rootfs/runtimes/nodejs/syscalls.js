const vsock = require("vsock");
const syscalls_pb = require("./syscalls_pb")

class Syscall {
    constructor(sock) {
        this.sock = sock;
    }

    async _send(obj) {
        const objData = obj.serializeBinary(); // UInt8Array
        const buf = Buffer.alloc(4 + objData.length);
        let offset = buf.writeUInt32BE(objData.length);
        for (const b of objData) {
            offset = buf.writeUInt8(b, offset);
        }
        return await this.sock.write(buf);
    }

    async _recv(obj) {
        const lenBuf = await this.sock.read(4);
        const len = lenBuf.readUInt32BE();
        const objBuff = await this.sock.read(len);
        return obj
            .constructor
            .deserializeBinary(objBuff);
    }

    async request() {
        const request = new syscalls_pb.Request();
        return await this._recv(request);
    }

    async respond(res) {
        const _response = new syscalls_pb.Response();
        _response.setPayload(JSON.stringify(res));
        const response = new syscalls_pb.Syscall();
        response.setResponse(_response);
        await this._send(response);

        // const response = new syscalls_pb.Syscall();
        // response.setResponse(
            // (() => {
                // const response = new syscalls_pb.Response();
                // response.setPayload(JSON.stringify(res));
                // return response;
            // })()
        // );
        // await this._send(response);
    }

    async write_key(key, value) {
        const writeKey = new syscalls_pb.WriteKey();
        writeKey.setKey(key);
        writeKey.setValue(value);
        const req = new syscalls_pb.Syscall();
        req.setWritekey(writeKey);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.WriteKeyResponse());
        return response.getSuccess();

        // const req = new syscalls_pb.Syscall();
        // req.setWritekey(
            // (() => {
                // const writeKey = new syscalls_pb.WriteKey();
                // writeKey.setKey(key);
                // writeKey.setValue(value);
                // return writeKey;
            // })()
        // );
        // await this._send(req);
        // const response =
            // await this._recv(new syscalls_pb.WriteKeyResponse());
        // return response.getSuccess();
    }

    async read_key(key) {
        const readKey = new syscalls_pb.ReadKey();
        readKey.setKey(key);
        const req = new syscalls_pb.Syscall();
        req.setReadkey(readKey);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.ReadKeyResponse());
        return response.getValue();

        // const req = new syscalls_pb.Syscall();
        // req.setReadkey(
            // (() => {
                // const readKey = new syscalls_pb.ReadKey();
                // readKey.setKey(key);
                // return readKey;
            // })()
        // );
        // await this._send(req);
        // const response =
            // await this._recv(new syscalls_pb.ReadKeyResponse());
        // return response.getValue();
    }
}
module.exports.Syscall = Syscall;
