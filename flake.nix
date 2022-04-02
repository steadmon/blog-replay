{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=release-21.05";
    cargo2nix.url = "github:cargo2nix/cargo2nix/master";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.flake-utils.follows = "flake-utils";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, cargo2nix, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [(import "${cargo2nix}/overlay")
                      rust-overlay.overlay];
        };

        rustPkgs = pkgs.rustBuilder.makePackageSet' {
          rustChannel = "1.58.1";
          packageFun = import ./Cargo.nix;
        };

      in rec {
        packages = {
          blog-replay = (rustPkgs.workspace.blog-replay {}).bin;
        };

        defaultPackage = packages.blog-replay;
      }
    );
}
