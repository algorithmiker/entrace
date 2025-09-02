{
  lib,
  breakpointHook,
  libGL,
  libxkbcommon,
  luajit,
  mold-wrapped,
  patchelf,
  pkg-config,
  rustPlatform,
  wayland,
}:
let
  extraRPATHLibs = [
    libGL
    libxkbcommon
    wayland
  ];
in
rustPlatform.buildRustPackage {
  pname = "entrace";
  version = "0.1.0";
  buildInputs = [
    libxkbcommon
    luajit
    wayland
  ];
  nativeBuildInputs = [
    breakpointHook
    mold-wrapped
    patchelf
    pkg-config
  ];
  src = lib.cleanSource ./.;
  postFixup = ''
    ENTRACE_BIN="$out/bin/entrace"
    patchelf --add-rpath ${lib.makeLibraryPath extraRPATHLibs} "$ENTRACE_BIN"
    patchelf --add-needed libwayland-client.so --add-needed libxkbcommon.so --add-needed libEGL.so "$ENTRACE_BIN"
  '';
  env.RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
  cargoHash = "sha256-3yLmsBJbptRBMkpB1Odt6WzIlCmMAgOVn03Ho6gzFO8=";
  meta = {
    mainProgram = "entrace";
  };
}
