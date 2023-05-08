{ pkgs ? import <nixpkgs> {} }:

with pkgs;

mkShell {
  buildInputs = [ rustup cargo rustc rustfmt capnproto protobuf pkg-config openssl unzip cmake];
}
