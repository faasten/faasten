{ pkgs ? import <nixpkgs> {}, release ? true }:

with pkgs;
builtins.mapAttrs (name: crate: crate.build.override {
  crateOverrides = defaultCrateOverrides // {
    prost-build = attrs: {
      buildInputs = [ pkgs.protobuf ];
    };
  };
}) (import ./Cargo.nix { inherit pkgs release; }).workspaceMembers
