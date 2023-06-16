{ pkgs ? import <nixpkgs> {} }:

with pkgs;

mkShell {
  buildInputs = [ rustup cargo rustc rustfmt protobuf3_19 pkg-config openssl unzip cmake squashfs-tools-ng python3 python3Packages.pip ];
}
