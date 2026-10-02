{
  description = "textures.gg crates and desktop app: development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      fenix,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];
      forEachSystem = nixpkgs.lib.genAttrs systems;
      devShellFor =
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          inherit (pkgs.stdenv.hostPlatform) isLinux;
          fenixPackages = fenix.packages.${system};
          rustToolchain = fenixPackages.combine [
            (fenixPackages.stable.withComponents [
              "cargo"
              "clippy"
              "rust-src"
              "rustc"
              "rustfmt"
            ])
            # The renderer and the crates beneath it also build for the web.
            fenixPackages.targets.wasm32-unknown-unknown.stable.rust-std
          ];
          # What gpui-ce and wgpu link against. macOS system frameworks
          # (Metal, AppKit) come from nixpkgs' Apple SDK.
          nativeLibraries =
            with pkgs;
            [
              fontconfig
              freetype
              openssl
              zlib
            ]
            ++ lib.optionals isLinux [
              alsa-lib
              libxkbcommon
              vulkan-loader
              wayland
              libxcb
            ];
        in
        pkgs.mkShell (
          {
            packages = [
              rustToolchain
              pkgs.git
              pkgs.pkg-config
              pkgs.rust-analyzer
            ];

            buildInputs = nativeLibraries;
            RUST_BACKTRACE = "1";
          }
          // pkgs.lib.optionalAttrs isLinux {
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath nativeLibraries;
          }
        );
    in
    {
      devShells = forEachSystem (system: {
        default = devShellFor system;
      });
    };
}
