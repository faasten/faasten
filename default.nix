{ pkgs ? import <nixpkgs> {}} :

pkgs.rustPlatform.buildRustPackage rec {
  pname = "faasten";
  version = "0.1.0";

  buildType = "debug";

  src = ./.;

  cargoSha256 = "sha256-QbbLOZctTllrFXCiloB9yUjrTXbBPY5z82VJFziWiE0=";

  nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
  buildInputs = [ pkgs.openssl ];

  meta = {
    description = "A user-centric function-as-a-service platform";
    homepage = "https://github.com/faasten/faasten";
  };
}
