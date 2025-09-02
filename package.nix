{
  lib,
  luajit,
  pkg-config,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "entrace";
  version = "0.1.0";
  buildInputs = [
    luajit
  ];
  nativeBuildInputs = [ pkg-config ];
  src = lib.cleanSource ./.;
  cargoHash = "sha256-3yLmsBJbptRBMkpB1Odt6WzIlCmMAgOVn03Ho6gzFO8=";
  meta = {
    mainProgram = "entrace_gui";
  };
}
