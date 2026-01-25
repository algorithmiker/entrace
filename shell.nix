let
  pinnedPkgsWithFetcher =
    fetchFromGitHub:
    fetchFromGitHub {
      owner = "nixos";
      repo = "nixpkgs";
      rev = "523257564973361cc3e55e3df3e77e68c20b0b80";
      #branch = "nixos-unstable";
      hash = "sha256-saOixpqPT4fiE/M8EfHv9I98f3sSEvt6nhMJ/z0a7xI=";
    };
  pathPkgs = import <nixpkgs> { };
in
{
  pkgs ? import (pinnedPkgsWithFetcher (pathPkgs.fetchFromGitHub)) { },
  rustfmt-nightly ? pkgs.rustfmt,
}:
let
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
    pkgs.dioxus-cli
    pkgs.cargo-semver-checks
  ];
  preferLocalBuild = true;
  env.RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath libPathPackages}"
  '';
}
