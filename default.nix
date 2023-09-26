{ pkgs ? import <nixpkgs> {}} :

pkgs.rustPlatform.buildRustPackage rec {
  pname = "faasten";
  version = "0.1.0";

  buildType = "release";

  src = builtins.filterSource
      (path: type: !(type == "directory" && baseNameOf path == "target"))
          ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      "arch-0.1.0" = "sha256-NCqWlwj88NH/zU1tFO6+0dYsMaSCHHj9mzsGNuox9O8=";
      "kvm-bindings-0.1.1" = "sha256-gqFUe8cFKcmS3uoFEf4wlMSQidXMR11pSU5tDqBDa9k=";
      "labeled-0.1.0" = "sha256-cyUXSHC7kN2MayV+FQCSL0hQHTQXv+YQMiCLpTsFuTY=";
    };
  };

  nativeBuildInputs = [ pkgs.perl pkgs.gcc10 pkgs.openssl pkgs.pkg-config pkgs.protobuf pkgs.unzip pkgs.cmake ];
  buildInputs = [ pkgs.openssl ];

  meta = {
    description = "A user-centric function-as-a-service platform";
    homepage = "https://github.com/faasten/faasten";
  };
}
