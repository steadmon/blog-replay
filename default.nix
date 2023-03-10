{ pkgs, naersk, ... }:
let
  naerskLib = pkgs.callPackage naersk {};
in {
  blog-replay = naerskLib.buildPackage {
    name = "blog-replay";
    src = ./.;
    nativeBuildInputs = [ pkgs.pkg-config ];
    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
  };
}
