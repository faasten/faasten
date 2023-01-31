# Faasten
## Frontend
- [ ] add user-facing API for interactions with Faasten file system
- [ ] dynamic function registration API
  - [ ] functions can be registered dynamically, instead of from a YAML configuration file
  - [ ] automate function packaging, i.e., converting the source into Faasten function image
- [ ] self-hosting
  - [ ] store function images as blobs in Faasten file system
  - [ ] makes function gates link image blobs
## Core
- [ ] low-level system calls
  - [ ] opaque handles (branch `opaque-handles` sketched something)
  - [ ] system calls
- [ ] generic gates
  - [ ] allow access to external data sinks/sources
- [ ] garbage collection
  - [ ] implementation
  - [ ] measuring garbage collection performance overhead
  - [ ] ...
- [ ] distributed scheduler
- [ ] distributed storage
- [ ] VM caching
  - [ ] If the scheduler is unaware of labels, then for a worker thread what to do with a cached tainted VM.
# Faasten application
- [ ] grader
- [ ] photo management
- [ ] Deathstar
