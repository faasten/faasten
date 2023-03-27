{ pkgs ? import <nixpkgs> {}} :

pkgs.rustPlatform.buildRustPackage rec {
  pname = "faasten";
  version = "0.1.0";

  src = ./.;

  cargoSha256 = "sha256-Uqk+FD8ve37TMA6h0hnaV5aoIDrsRed2pKQLtXAtzBk=";

  nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
  buildInputs = [ pkgs.openssl ];

  meta = {
    description = "A user-centric function-as-a-service platform";
    homepage = "https://github.com/faasten/faasten";
  };
}
