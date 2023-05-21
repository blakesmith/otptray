{ pkgs,
  lib,
  rustPlatform,
  llvmPackages
}:

with rustPlatform;

buildRustPackage rec {
  pname = "otptray";
  version = "0.1.0";
  src = builtins.filterSource
    (path: type: type != "directory" || baseNameOf path != "target")
    ./.;
  cargoSha256 = "s5W5l+C1cuGTO9nHBkWPe0IfWIKO1+nn+HUp6ua61Wg=";
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


  # Update all gnome desktop application binary references
  postPatch = ''
  substituteInPlace share/applications/otptray.desktop \
    --replace /usr $out
  '';

  installPhase = ''
  mkdir -p $out/bin $out/share
  cp -rv share $out/
  find target/ -name "otptray" -exec cp '{}' $out/bin \;
  '';
  
  meta = with lib; {
    description = "OTPTray 2FA / OTP helper appindicator tray application";
    homepage = "https://blakesmith.me";
    license = with licenses; [ mit ];
  };
}
