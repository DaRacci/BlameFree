{
  description = "BlameFree flake";

  nixConfig = {
    extra-substituters = [
      "https://cache.nixos.org"
      "https://nix-community.cachix.org"
      "https://devenv.cachix.org"
    ];
    extra-trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
      "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
    ];
  };

  inputs = {
    devenv-root = {
      url = "file+file:///dev/null";
      flake = false;
    };
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    devenv.url = "github:cachix/devenv";
    treefmt.url = "github:numtide/treefmt-nix";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.devenv.flakeModule
        inputs.treefmt.flakeModule
      ];

      debug = true;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem =
        {
          config,
          pkgs,
          lib,
          ...
        }:
        let
          buildInputs = with pkgs; [ pkg-config ];
        in
        {
          devenv.shells.default = {
            containers = lib.mkForce { };

            languages.rust = {
              enable = true;
              toolchainFile = ./rust-toolchain.toml;
            };

            packages =
              with pkgs;
              [
                cargo-expand
                cargo-insta
                cargo-leptos
                cargo-license
                cargo-nextest
                cargo-sort
                trunk
                wasm-bindgen-cli
              ]
              ++ buildInputs
              ++ config.treefmt.build.devShell.buildInputs;

            env = {
              LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
              RUST_BACKTRACE = 1;
            };

            git-hooks.hooks = {
              clippy.enable = true;
              treefmt = {
                enable = true;
                packageOverrides.treefmt = config.treefmt.build.wrapper;
              };
            };
          };

          treefmt = {
            projectRootFile = ".git/config";
            programs = {
              nixfmt.enable = true;
              rustfmt.enable = true;
            };
          };
        };
    };
}
