{ pkgs ? import <nixpkgs> {}} :

pkgs.rustPlatform.buildRustPackage rec {
  pname = "faasten";
  version = "0.1.0";

  src = ./.;

  cargoSha256 = "sha256-aOXzWHiVHq7B/IuP/XBIl6RDunx1mUqVF4oGlLUfURA=";

  nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
  buildInputs = [ pkgs.openssl ];

  meta = {
    description = "A user-centric function-as-a-service platform";
    homepage = "https://github.com/faasten/faasten";
  };
}
