{
  # pkgs is pinned in the flake
  pkgs ? import <nixpkgs> { },
  rustfmt-nightly ? pkgs.rustfmt,
}:
let
  # https://github.com/DioxusLabs/dioxus/issues/5540#issuecomment-4433970098
  # needed for dioxus-cli 0.7.5 with working hot reload
  nixpkgsOld = import (pkgs.fetchFromGitHub {
    owner = "nixos";
    repo = "nixpkgs";
    rev = "01fbdeef22b76df85ea168fbfe1bfd9e63681b30";
    hash = "sha256-GMSVw35Q+294GlrTUKlx087E31z7KurReQ1YHSKp5iw=";
  }) { system = pkgs.system; };

  libPathPackages = [
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.libGL
    pkgs.dbus.lib
  ];
in
pkgs.mkShell {
  packages = libPathPackages ++ [
    pkgs.sqlite
    pkgs.mold-wrapped
    pkgs.pkg-config
    pkgs.luajit
    pkgs.zenity
    rustfmt-nightly
    pkgs.cargo-about
    nixpkgsOld.dioxus-cli
    pkgs.cargo-semver-checks
  ];
  preferLocalBuild = true;
  env.RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath libPathPackages}"
  '';
}
