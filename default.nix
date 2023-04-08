{ pkgs ? import <nixpkgs> {}} :

with pkgs;

rustPlatform.buildRustPackage rec {
  pname = "faasten";
  version = "0.1.0";

  buildType = "release";

  src = ./.;

  cargoSha256 = "sha256-+ppmgEB0w11FFV1Tb1WCoc+A3kfGH/TxQ4tvej3b0mc=";

  doCheck = false;

  buildInputs = [ openssl pkg-config protobuf unzip cmake ];
  nativeBuildInputs = [ openssl pkg-config protobuf unzip cmake ];

  meta = {
    description = "A user-centric function-as-a-service platform";
    homepage = "https://github.com/faasten/faasten";
  };
}
