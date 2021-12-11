# snapfaas as a library
## API
1. Developers must implement trait app::Handle to process a HTTP request and return a SnapFaaS request
on success for push event and `None` for ping event and HTTP status code on error.
```rust
app::Handler::handle_request(&mut self, request: &http::Request<Bytes>) -> Result<request::Request, http::StatusCode>
```
1. An example webhook server is in `bins/webhook`.
```rust
# create a new server of certain memory for running VMs and listening at `listen_addr` with configuration
# at path `config_path`.
# The server runs application `handler` which implements the `snapfaas::app::Handler` trait
use snapfaas::server::Server;
let s = Server::new(total_mem, config_path, listen_addr, handler);
s.run()
```
