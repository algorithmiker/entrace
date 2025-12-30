let
  pinnedPkgsWithFetcher =
    fetchFromGitHub:
    fetchFromGitHub {
      owner = "nixos";
      repo = "nixpkgs";
      rev = "c6245e83d836d0433170a16eb185cefe0572f8b8";
      #branch = "nixos-unstable";
      hash = "sha256-G/WVghka6c4bAzMhTwT2vjLccg/awmHkdKSd2JrycLc=";
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
  ];
  preferLocalBuild = true;
  env.RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath libPathPackages}"
  '';
}
