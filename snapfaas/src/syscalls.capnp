@0x85150b117366d14b;

struct Buckle {
  using Principal = List(Text);
  using Clause = List(Principal);
  struct Component {
    union {
      false @0 :Void;
      component @1 :List(Clause);
    }
  }

  secrecy   @0 :Component;
  integrity @1 :Component;
}

struct PathComponent {
  union {
    dscrp @0 :Text;
    facet @1 :Buckle;
  }
}

const public :Buckle = (secrecy = (component = []), integrity = (component = []));
const bottom :Buckle = (secrecy = (component = []), integrity = (false = void));
const top :Buckle = (secrecy = (false = void), integrity = (component = []));

struct Fd(T) {
  fd @0 :Int64;
}

struct File {
  label @0 :Buckle;
  data  @1 :Data;
}

struct OpenBlob {
}

struct Blob {
}

struct Service {
}

struct Gate {
}

struct Faceted {
}

struct DirEntry {
  union {
    dir     @0 :Fd(Dir);
    faceted @1 :Fd(Faceted);
    file    @2 :Fd(File);
    blob    @3 :Fd(Blob);
    gate    @4 :Fd(Gate);
    service @5 :Fd(Service);
  }
}

struct Dir {
  label    @0 :Buckle;
  entries  @1 :List(DirEntry);
}

struct Optional(T) {
  union {
    error @0 :Void;
    result @1 :T;
  }
}

interface CloudCall {
  # Lifecycle
  respond @0 (payload :Data) -> ();

  # Labels
  getCurrentLabel @1 () -> (label: Buckle);
  taintWithLabel @2 (label: Buckle) -> (label: Buckle);

  # File system
  createFile     @3 (label :Buckle) -> (file :Fd(File));
  createBlob     @4 (label :Buckle) -> (blob :Fd(OpenBlob));
  createDir      @5 (label :Buckle) -> (dir :Fd(Dir));
  createFaceted  @6 (label :Buckle) -> (faceted :Fd(Faceted));

  openAt         @7 (dir :Fd(Dir), path :Text) -> (entry :Optional(DirEntry));
  openAtFaceted  @8 (dir :Fd(Dir), facet :Buckle) -> (entry :Optional(DirEntry));

  list           @9 (dir :Fd(Dir)) -> (dir: Dir);
  # facetedList ??
  write         @10 (fd :Fd(File), data :Data);
  read          @11 (fd :Fd(File)) -> (data :Data);
  append        @12 (fd :Fd(OpenBlob), data :Data);
  finalize      @13 (fd :Fd(OpenBlob)) -> (blob :Fd(Blob));
  readBlob      @14 (fd :Fd(Blob), offset :Int64, length :Int64) -> (data :Data);

  link          @15 (dir :Fd(Dir), name :Text, entry :DirEntry);
  unlink        @16 [T] (dir :Fd(Dir), name :Text) -> (entry :Optional(DirEntry));
  # facetedLink
  # facetedUnlink

  invokeGate    @17 (gate :List(PathComponent), payload :Data);
}
