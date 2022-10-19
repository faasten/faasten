#include <napi.h>
#include <unistd.h>
#include <sys/socket.h>
#include <linux/vm_sockets.h>

static int vsock_connect(int cid, int port)
{
	int fd;
	struct sockaddr_vm sa = {
    .svm_family =  AF_VSOCK,
  };
	sa.svm_cid = cid;
	sa.svm_port = port;

	fd = socket(AF_VSOCK, SOCK_STREAM, 0);
	if (fd < 0) {
		perror("socket");
		return -1;
	}

	if (connect(fd, (struct sockaddr*)&sa, sizeof(sa)) != 0) {
		perror("connect");
		close(fd);
		return -1;
	}

	return fd;
}

Napi::Value Read(const Napi::CallbackInfo& info) {
  unsigned int fd = info[0].As<Napi::Number>();
  Napi::Buffer<char> buffer = info[1].As<Napi::Buffer<char>>();

  int s = read(fd, buffer.Data(), buffer.Length());
  if (s < -1) {
		perror("read");
  }
  return Napi::Number::New(info.Env(), s);
}

// This method has access to the data stored in the environment because it is
// an instance method of `VsockAddon` and because it was listed among the
// property descriptors passed to `DefineAddon()` in the constructor.
Napi::Value Connect(const Napi::CallbackInfo& info) {
  unsigned int cid = info[0].As<Napi::Number>();
  unsigned int port = info[1].As<Napi::Number>();
  int fd = vsock_connect(cid, port);
  if (fd < 0) {
    perror("connect vsock");
  }
  return Napi::Number::New(info.Env(), fd);
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
  exports.Set(Napi::String::New(env, "connect"), Napi::Function::New(env, Connect));
  exports.Set(Napi::String::New(env, "read"), Napi::Function::New(env, Read));
  return exports;
}

NODE_API_MODULE(addon, Init)
