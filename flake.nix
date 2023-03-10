{
  description = "A utility to replay a blog's archive into an Atom feed";
  inputs = {
    naersk.url = "github:nix-community/naersk";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.follows = "rust-overlay/flake-utils";
    nixpkgs.follows = "rust-overlay/nixpkgs";
  };

  outputs = inputs: with inputs;
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        code = pkgs.callPackage ./. { inherit pkgs naersk; };
      in rec {
        packages = {
          blog-replay = code.blog-replay;
          default = packages.blog-replay;
        };

        defaultPackage = packages.blog-replay;
      }
    );
}
