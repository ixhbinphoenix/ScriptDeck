{
  description = "A basic flake with a shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};

      libraries = with pkgs; [
        webkitgtk
        gtk3
        cairo
        gdk-pixbuf
        glib
        dbus
        openssl_3
        librsvg
        libthai
      ];
    in {
      devShells.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          rustup
          tauri-mobile
          cargo-leptos
          leptosfmt
          trunk
          pkg-config
          libsoup
          webkitgtk
          appimagekit
          librsvg
        ];

        shellHook = ''
          export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath libraries}:$LD_LIBRARY_PATH
          rustup default nightly
          rustup target add wasm32-unknown-unknown
          rustup component add rust-analyzer
          cargo help tauri 2>/dev/null 1>/dev/null || cargo install tauri-cli
        '';
      };
    });
}
