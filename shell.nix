# Reproducible dev shell for the Wordle solver on NixOS (and any machine with
# Nix). Provides the Rust toolchain plus the native bits reqwest needs at link
# time: a C compiler comes from mkShell's stdenv, and openssl + pkg-config
# satisfy reqwest's default (native-tls) TLS backend. Activated by .envrc via
# direnv when Nix is available.
{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
  ];

  buildInputs = with pkgs; [
    rustc
    cargo
    clippy
    rustfmt
    rust-analyzer
    openssl
  ];

  # Help pkg-config / openssl-sys find the system openssl inside the shell.
  PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
}
