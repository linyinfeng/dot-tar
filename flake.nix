{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils-plus.url = "github:gytis-ivaskevicius/flake-utils-plus";
  };

  outputs = inputs@{ self, nixpkgs, flake-utils-plus }:
    let
      utils = flake-utils-plus.lib;
    in
    utils.mkFlake {
      inherit self inputs;

      channels.nixpkgs.input = nixpkgs;
      outputsBuilder = channels:
        let
          pkgs = channels.nixpkgs;
        in
        rec {
          packages.dot-tar = pkgs.callPackage ./dot-tar.nix { };
          defaultPackage = packages.dot-tar;

          apps.dot-tar = utils.mkApp {
            drv = packages.dot-tar;
          };
          defaultApp = apps.dot-tar;

          devShell = pkgs.mkShell {
            packages = with pkgs; [
              rustup
              pkg-config
              openssl
            ];
          };
        };

      nixosModules = {
        dot-tar = import ./modules/services/dot-tar.nix;
      };
      overlay = self.overlays.dot-tar;
      overlays = {
        dot-tar = final: prev: {
          dot-tar = self.packages.${final.system}.dot-tar;
        };
      };
    };
}
