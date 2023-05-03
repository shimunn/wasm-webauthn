{
  description = "A naersk based rust flake";

  inputs = {
    utils.url = "github:numtide/flake-utils";
    naersk = {
      url = "github:nmattia/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fenix.url = "github:nix-community/fenix";
  };

  outputs = inputs @ { self, nixpkgs, utils, naersk, ... }:
    let
      root = inputs.source or self;
      pname = "wasm-webauthn";
      # toolchains: stable, beta, default(nightly)
      toolchain = pkgs: with inputs.fenix.packages."${pkgs.system}"; combine [
        minimal.rustc
        minimal.cargo
        latest.rust-std
        targets.wasm32-unknown-unknown.latest.rust-std
      ];
      forSystem = system:
        let
          pkgs = nixpkgs.legacyPackages."${system}";
        in
        rec {
          # `nix flake check`
          checks = {
            fmt = with pkgs; runCommandLocal "${pname}-fmt" { buildInputs = [ cargo rustfmt nixpkgs-fmt ]; } ''
              cd ${root}
              cargo fmt -- --check
              nixpkgs-fmt --check *.nix
              touch $out
            '';
          };

          # `nix develop`
          devShell = pkgs.mkShell rec {
            RUST_SRC_PATH = "${if inputs ? fenix then "${toolchain pkgs}/lib/rustlib" else pkgs.rustPlatform.rustLibSrc}";
            RUST_LOG = "debug";

            SSH_CD_SOCKET_ADDRESS = "127.0.0.1:6869";
            SSH_CD_CERT_DIR = "certs/";
            SSH_CD_CA = "certs/ca.pub";

            SSH_CD_API = "http://${SSH_CD_SOCKET_ADDRESS}";

            nativeBuildInputs = with pkgs; [ (toolchain pkgs) cargo-watch rustfmt nixpkgs-fmt bacon wasm-pack trunk nodePackages.sass ];
            shellHook = ''
              printf "Rust version:"
              rustc --version
              printf "\nbuild inputs: ${pkgs.lib.concatStringsSep ", " (map (bi: bi.name) (nativeBuildInputs))}"
            '';
          };

        };
    in
    (utils.lib.eachDefaultSystem forSystem);

}
