# Snapfaas as a Library
## API
1. App's logic goes into its trait `snapfaas::app::Handle` implementation.
```rust
pub struct App {
  internal_state: SomeState,
}

impl App {
  pub fn new() {
    // initialization code
  }
}

impl snapfaas::app::Handler for App {
  pub fn handle_request(&mut self, request: &http::Request<Bytes>) -> Result<request::Request, http::StatusCode> {
    //app's logic...
  }
}
```
2. To create a server
```rust
# create a new server of certain memory for running VMs and listening at `listen_addr` with configuration
# at path `config_path`.
let handler = App::new();
let s = Server::new(total_mem, config_path, listen_addr, handler);
s.run()
```
## Example
An example webhook server is in `bins/webhook`.
