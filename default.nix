{ pkgs ? import <nixpkgs> {}} :

pkgs.rustPlatform.buildRustPackage rec {
  pname = "faasten";
  version = "0.1.0";

  buildType = "debug";

  src = ./.;

  cargoSha256 = "sha256-DmgQHMmTBYnCcFVEn5HYFPXwor83wYLZ9NudFiOmDeQ=";

  nativeBuildInputs = [ pkgs.perl pkgs.openssl pkgs.pkg-config pkgs.protobuf pkgs.unzip pkgs.cmake ];

  meta = {
    description = "A user-centric function-as-a-service platform";
    homepage = "https://github.com/faasten/faasten";
  };
}
