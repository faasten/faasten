{ pkgs ? import <nixpkgs> {}} :

pkgs.rust_1_66.packages.stable.buildRustPackages.rustPlatform.buildRustPackage rec {
  pname = "faasten";
  version = "0.1.0";

  buildType = "release";

  src = ./.;

  cargoSha256 = "sha256-RRqiD91FcLRRkaNxMmqoSLBitxbEw23/dCOxvpoOvIw=";

  nativeBuildInputs = [ pkgs.perl pkgs.openssl pkgs.pkg-config pkgs.protobuf pkgs.unzip pkgs.cmake ];

  meta = {
    description = "A user-centric function-as-a-service platform";
    homepage = "https://github.com/faasten/faasten";
  };
}
