{ pkgs,
  lib,
  rustPlatform,
  llvmPackages
}:

with rustPlatform;

buildRustPackage rec {
  pname = "otptrap";
  version = "0.1.0";
  src = builtins.filterSource
    (path: type: type != "directory" || baseNameOf path != "target")
    ./.;
  cargoSha256 = "RwM+8txnToOuniTEagXjz0BtMvOjjyPbIuxBP7ic7Io=";
  nativeBuildInputs = [
    pkgs.clang
    pkgs.pkg-config
  ];
  buildInputs = [
    pkgs.libappindicator-gtk3
    pkgs.gtk3.dev
    llvmPackages.libclang
    # pkgs.glib.dev
  ];
  doCheck = false;

  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
  BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${llvmPackages.libclang.lib}/lib/clang/${lib.getVersion pkgs.clang}/include";

  meta = with lib; {
    description = "OTPTray 2FA / OTP helper appindicator tray application";
    homepage = "https://blakesmith.me";
    license = with licenses; [ mit ];
  };
}
