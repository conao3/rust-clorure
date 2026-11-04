{
  description = "Rust development environment for rust-clorure";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-darwin" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
        lib = pkgs.lib;
      in
      {
        devShells.default = pkgs.mkShell {
          packages =
            with pkgs;
            [
              cargo
              cargo-watch
              clippy
              pkg-config
              rust-analyzer
              rustc
              rustfmt
            ]
            ++ lib.optionals stdenv.hostPlatform.isLinux [
              mold
            ]
            ++ lib.optionals stdenv.hostPlatform.isDarwin [
              libiconv
            ];

          shellHook = ''
            export RUST_SRC_PATH="${pkgs.rustPlatform.rustLibSrc}"
            echo "Rust development shell ready"
          '';
        };
      });
}
