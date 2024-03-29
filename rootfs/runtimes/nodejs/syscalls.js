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
        const readDir = new syscalls_pb.ReadDir();
        readDir.setDir(Buffer.from(d, "utf-8"));
        const req = new syscalls_pb.Syscall();
        req.setReaddir(readDir);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.ReadDirResponse());
        const l = response.getKeysList();
        return l.map(b => Buffer.from(b).toString("utf-8"));
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
    async declassify(secrecy) { /* TODO */ }
    /* End of Label APIs */

    /* Github APIs */
    async github_rest_get(route, token, toblob=false) {
        const rest = new syscalls_pb.GithubRest();
        rest.setVerb(syscalls_pb.HttpVerb.GET);
        rest.setRoute(route);
        rest.setBody(null);
        rest.setToblob(toblob);
        rest.setToken(token);
        const req = new syscalls_pb.Syscall();
        req.setGithubrest(rest);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.GithubRestResponse());
        return response;
    }

    async github_rest_post(route, body, token, toblob=false) {
        const bodyJson = JSON.stringify(body);
        const rest = new syscalls_pb.GithubRest();
        rest.setVerb(syscalls_pb.HttpVerb.POST);
        rest.setRoute(route);
        rest.setBody(bodyJson);
        rest.setToblob(toblob);
        rest.setToken(token);
        const req = new syscalls_pb.Syscall();
        req.setGithubrest(rest);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.GithubRestResponse());
        return response;
    }

    async github_rest_put(route, body, token, toblob=false) {
        const bodyJson = JSON.stringify(body);
        const rest = new syscalls_pb.GithubRest();
        rest.setVerb(syscalls_pb.HttpVerb.PUT);
        rest.setRoute(route);
        rest.setBody(bodyJson);
        rest.setToblob(toblob);
        rest.setToken(token);
        const req = new syscalls_pb.Syscall();
        req.setGithubrest(rest);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.GithubRestResponse());
        return response;
    }

    async github_rest_delete(route, body, token, toblob=false) {
        const bodyJson = JSON.stringify(body);
        const rest = new syscalls_pb.GithubRest();
        rest.setVerb(syscalls_pb.HttpVerb.DELETE);
        rest.setRoute(route);
        rest.setBody(bodyJson);
        rest.setToblob(toblob);
        rest.setToken(token);
        const req = new syscalls_pb.Syscall();
        req.setGithubrest(rest);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.GithubRestResponse());
        return response;
    }
    /* End of Github APIs */


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

    async fs_read(p) {
        const fsRead = new syscalls_pb.FSRead();
        fsRead.setPath(p);
        const req = new syscalls_pb.Syscall();
        req.setFsread(fsRead);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.ReadKeyResponse());
        return response.getValue();
    }

    async fs_write(p, data) {
        const fsWrite = new syscalls_pb.FSWrite();
        fsWrite.setPath(p);
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
    async fs_createdir(p, label=null) {
        const dir = path.dirname(p);
        const name = path.basename(p);
        const fsDir = new syscalls_pb.FSCreateDir();
        fsDir.setBasedir(dir);
        fsDir.setName(name);
        fsDir.setLabel(label);
        const req = new syscalls_pb.Syscall();
        req.setFscreatedir(fsDir);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.WriteKeyResponse())
        return response.getSuccess();
    }

    /**
     * @type {syscalls_pb.DcLabel} label
     */
    async fs_createfile(p, label=null) {
        const dir = path.dirname(p);
        const name = path.basename(p);
        const fsFile = new syscalls_pb.FSCreateFile();
        fsFile.setBasedir(dir);
        fsFile.setName(name);
        fsFile.setLabel(label);
        const req = new syscalls_pb.Syscall();
        req.setFscreatefile(fsFile);
        await this._send(req);
        const response =
            await this._recv(new syscalls_pb.WriteKeyResponse())
        return response.getSuccess();
    }


}
module.exports.Syscall = Syscall;


class NewBlob {
    constructor(fd, syscall) {
        this.fd = fd;
        this.syscall = syscall;
    }

    async write(data) {
        const blob = new syscalls_pb.BlobWrite();
        blob.setFd(this.fd);
        blob.setData(data);
        const req = new syscalls_pb.Syscall();
        req.setWriteblob(blob);
        await this.syscall._send(req);
        const response =
            await this.syscall._recv(new syscalls_pb.BlobResponse());
        return response.getSuccess();
    }

    async finalize(data) {
        const blob = new syscalls_pb.BlobFinalize();
        blob.setFd(this.fd);
        blob.setData(data);
        const req = new syscalls_pb.Syscall();
        req.setFinalizeblob(blob);
        await this.syscall._send(req);
        const response =
            await this.syscall._recv(new syscalls_pb.BlobResponse());
        const buf = Buffer.from(response.getData());
        return buf.toString("utf-8");
    }
}

class Blob {
    constructor(fd, syscall) {
        this.fd = fd;
        this.syscall = syscall;
    }

    async _blob_read(offset=null, length=null) {
        const blob = new syscalls_pb.BlobRead();
        blob.setFd(this.fd);
        blob.setOffset(offset);
        blob.setLength(length);
        const req = new syscalls_pb.Syscall();
        req.setReadblob(blob);
        await this.syscall._send(req);
        const response =
            await this.syscall._recv(new syscalls_pb.BlobResponse());
        if (response.getSuccess()) {
            return response.getData(); // TODO or getData_asU8?
        } else {
            // TODO error handling
        }
    }

    async read(size=null) {
        let buf = Buffer.alloc(0);
        if (size == null) {
            return await this._blob_read();
        } else {
            while (size > 0) {
                const data = await this._blob_read(size);
                // reaches EOF
                if (data.length == 0) {
                    return buf;
                }
                // assume data is an u8 array
                buf = Buffer.concat(buffer, data);
                offset += data.length;
                size -= data.length;
            }
        }
        return buf;
    }
}















