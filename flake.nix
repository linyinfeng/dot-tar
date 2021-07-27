{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nmattia/naersk";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, utils, naersk, rust-overlay }:
    let name = "dot-tar"; in
    (utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlay ];
        };

        rust = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "rust-src" ];
        };
        naersk-lib = naersk.lib."${system}".override {
          cargo = rust;
          rustc = rust;
        };

        buildInputs = with pkgs; [
          openssl
        ];
        nativeBuildInputs = with pkgs; [
          pkg-config
        ];
      in
      rec {
        packages.${name} = naersk-lib.buildPackage {
          pname = name;
          root = ./.;

          inherit buildInputs nativeBuildInputs;
        };
        defaultPackage = packages.${name};

        apps.${name} = utils.lib.mkApp {
          drv = packages.${name};
        };
        defaultApp = apps.${name};

        devShell = pkgs.mkShell {
          inherit buildInputs nativeBuildInputs;
          packages = with pkgs; [
            rustup
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
