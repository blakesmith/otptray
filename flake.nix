{
  description = "OTPTray application";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/22.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
        rec {
          packages = {
            otptray = pkgs.callPackage ./. {};
          };
          defaultPackage = packages.otptray;
        }
    );
}
