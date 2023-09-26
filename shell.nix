{ pkgs ? import <nixpkgs> {} }:

with pkgs;

mkShell {
  name = "faasten-dev";
  buildInputs = [ rustup rustfmt protobuf pkg-config openssl squashfs-tools-ng foreman python3Packages.protobuf ];
}
