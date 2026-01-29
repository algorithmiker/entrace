{
  # pkgs is pinned in the flake
  pkgs ? import <nixpkgs> { },
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
