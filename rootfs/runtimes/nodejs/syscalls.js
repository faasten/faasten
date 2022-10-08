const vsock = require("vsock");
const path = require("path");
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
    }

    async read_dir(d) {
        // TODO see what type is it
        console.log(typeof d);
        console.log(d);
        const d = Buffer.from(d, "utf-8");
        const readDir = new syscalls_pb.ReadDir();
        readDir.setDir(d);
        const req = new syscalls_pb.Syscall();
        req.setReaddir(readDir);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.ReadDirResponse());
        const l = response.getKeysList(); // TODO is it array?
        return l.map(b => b.toString());
    }

    /* Label APIs */
    async get_current_label() {
        const label = new syscalls_pb.GetCurrentLabel();
        const req = new syscalls_pb.Syscall();
        req.setGetcurrentlabel(label);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.DcLabel());
        return response;
    }

    async taint(label) {
        const req = new syscalls_pb.Syscall();
        req.setTaintwithlabel(label);
        await this._send(req)
        const response =
            await this._recv(new syscalls_pb.DcLabel());
        return response;
    }

    /**
     * @type {syscalls_pb.Component} secrecy
     */
    // TODO
    async declassify(secrecy) {}
    /* End of Label APIs */

    async invoke(f, payload) {
        const invoke = new syscalls_pb.Invoke();
        invoke.setFunction(f);
        invoke.setPayload(payload);
        const req = new syscalls_pb.Syscall();
        req.setInvoke(invoke);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.InvokeResponse());
        return response.getSuccess();
    }

    async fs_read(path) {
        const fsRead = new syscalls_pb.FSRead();
        fsRead.setPath(path);
        const req = new syscalls_pb.Syscall();
        req.setFsread(fsRead);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.ReadKeyResponse());
        return response.getValue();
    }

    async fs_write(path, data) {
        const fsWrite = new syscalls_pb.FSWrite();
        fsWrite.setPath(path);
        fsWrite.setData(data);
        const req = new syscalls_pb.Syscall();
        req.setFswrite(fsWrite);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.WriteKeyResponse());
        return response.getSuccess();
    }

    /**
     * @type {syscalls_pb.DcLabel} label
     */
    async fs_createdir(path, label=null) {
        const dir = path.dirname(path);
        const name = path.basename(path);
        const fsDir = new syscalls_pb.FSCreateDir();
        fsDir.setBasedir(dir);
        fsDir.setName(name);
        fsDir.setLabel(label);
        const req = syscalls_pb.Syscall();
        req.setFscreatedir(fsDir);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.WriteKeyResponse())
        return response.getSuccess();
    }



}
module.exports.Syscall = Syscall;














