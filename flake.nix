{
  description = "terminal CLI for Kagi subscribers with JSON-first search output";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    let
      overlay = final: prev: {
        kagi = final.callPackage ./nix/kagi.nix { };
      };
    in
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ overlay ];
        };
      in
      {
        packages = {
          default = pkgs.kagi;
          kagi = pkgs.kagi;
        };

        apps.default = {
          type = "app";
          program = "${pkgs.kagi}/bin/kagi";
          meta.description = "Run the kagi CLI";
        };

        checks.kagi = pkgs.kagi;

        devShells.default = pkgs.mkShell {
          inputsFrom = [ pkgs.kagi ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };

        formatter = pkgs.nixfmt;
      }
    )
    // {
      overlays.default = overlay;
    };
}
