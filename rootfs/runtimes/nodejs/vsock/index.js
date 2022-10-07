const fs = require('fs');

const bindings = require('bindings')('vsock');

class Vsock {
  constructor(fd) {
    this.fd = fd;
    this.readStream = fs.createReadStream("", { fd: fd });
    this.writeStream = fs.createWriteStream("", { fd: fd });
    this.readBuffer = Buffer.alloc(0);
    this.readTarget = 0;
    this.readEvent = null;

    this.readStream.on('data', (chunk) => {
      this.readBuffer = Buffer.concat([this.readBuffer, chunk]);
      this.checkReadReady();
    });
  }

  checkReadReady() {
    if (this.readEvent && this.readBuffer.length >= this.readTarget) {
      const resBuf = this.readBuffer.slice(0, this.readTarget);
      this.readBuffer = this.readBuffer.slice(this.readTarget);
      const evt = this.readEvent;
      this.readEvent = null;
      process.nextTick(() => evt(resBuf));
    }
  }

  read(i) {
    return new Promise((resolve, reject) => {
      this.readEvent = resolve;
      this.readTarget = i;
      this.checkReadReady();
    });
  }

  write(chunk) {
    return new Promise((resolve, reject) => {
      if (this.writeStream.write(chunk)) {
        process.nextTick(resolve);
      } else {
        this.writeStream.once('drain', resolve);
      }
    });
  }
}
exports.Vsock = Vsock;

exports.connect = function(port, cid) {
  const fd = bindings.connect(port, cid);
  return new Vsock(fd);
}
exports.read = bindings.read;

exports.readRequest = async function(vs) {
  let len = 0;
  let lenBuf = await vs.read(4);
  len = lenBuf.readUInt32BE();
  let requestBuf = await vs.read(len);
  return JSON.parse(requestBuf);
}

exports.writeResponse = async function(vs, resp) {
  let respBuf = JSON.stringify(resp);
  let buf = Buffer.alloc(4 + respBuf.length);
  let offset = buf.writeUInt32BE(respBuf.length);
  buf.write(respBuf, offset);
  return await vs.write(buf);
}

