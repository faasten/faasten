{ pkgs ? import <nixpkgs> {} }:

with pkgs;

mkShell {
  buildInputs = [ rustup cargo rustc rustfmt protobuf ];
}
