{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let name = "dot-tar"; in
    (flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
      in
      rec {
        packages.${name} = pkgs.callPackage ./dot-tar.nix { };
        defaultPackage = packages.${name};

        apps.${name} = flake-utils.lib.mkApp {
          drv = packages.${name};
        };
        defaultApp = apps.${name};

        devShell = pkgs.mkShell {
          packages = with pkgs; [
            rustup
            pkg-config
            openssl
          ];
        };

        checks = packages;
      }))
    //
    rec {
      nixosModules = {
        dot-tar = import ./modules/services/dot-tar.nix;
      };
      overlay = overlays.dot-tar;
      overlays = {
        dot-tar = final: prev: {
          dot-tar = self.packages.${final.system}.dot-tar;
        };
      };
    };
}
