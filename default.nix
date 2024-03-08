{ pkgs, naersk, ... }:
let
  naerskLib = pkgs.callPackage naersk {};
in {
  blog-replay = naerskLib.buildPackage {
    name = "blog-replay";
    src = ./.;
    nativeBuildInputs = [ pkgs.pkg-config ];
    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
    meta = with pkgs.lib; {
      description = "A utility to replay a blog's archive into an Atom feed";
      longDescription = ''
        blog-replay scrapes articles from a given blog and stores them in a local Sled database. It
        can then gradually replay these articles into an Atom feed, which can be hosted on a
        website or consumed by a local feed reader.
      '';
      homepage = "https://github.com/steadmon/blog-replay";
      license = licenses.mit;
    };
  };
}
