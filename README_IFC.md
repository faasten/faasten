# snapfaas as a library
## API
1. Developers must implement trait app::Handle whose only member function consumes the TCP stream
and return a request ready to run in a VM.
```rust
app::Handler::handle_request(stream: TcpStream) -> Result<request::Request, io::Error>
```
1. The way to use the library:
```rust
# create a new server of certain memory for running VMs and listening at `listen_addr` with configuration
# at the path `config_path`. The server runs application `handler` which implements the snapfaas::app::Handler` trait
use snapfaas::server::Server;
let s = Server::new(total_mem, config_path, listen_addr, handler);
s.run()
```
