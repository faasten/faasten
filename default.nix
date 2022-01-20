{ pkgs ? import <nixpkgs> {}, release ? true }:

with pkgs;
((import ./Cargo.nix { inherit pkgs release; }).rootCrate.build).override {
  crateOverrides = defaultCrateOverrides // {
    prost-build = attrs: {
      buildInputs = [ pkgs.protobuf ];
    };
  };
}
