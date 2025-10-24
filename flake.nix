{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    fenix.url = "github:nix-community/fenix/monthly";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
      crane,
    }:
    let
      systems = nixpkgs.lib.systems.flakeExposed;
      forAllSystems = f: builtins.foldl' nixpkgs.lib.recursiveUpdate { } (builtins.map f systems);
    in
    forAllSystems (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustfmt-nightly = fenix.packages.${system}.default.rustfmt;
        craneLib = crane.mkLib pkgs;
        rpath-libs = [
          pkgs.libGL
          pkgs.libxkbcommon
          pkgs.wayland
          pkgs.luajit
        ];
        craneCommonArgs = {
          pname = "entrace";
          version = "0.1.1";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              (craneLib.fileset.commonCargoSources ./.)
              (pkgs.lib.fileset.fileFilter (file: file.hasExt "md") ./.)
              (pkgs.lib.fileset.maybeMissing ./gui/vendor)
              (pkgs.lib.fileset.maybeMissing ./docs)
            ];
          };
          buildInputs = [ ] ++ rpath-libs;
          nativeBuildInputs = [
            pkgs.breakpointHook
            pkgs.mold-wrapped
            pkgs.patchelf
            pkgs.pkg-config
          ];
        };
        cargoArtifacts = craneLib.buildDepsOnly craneCommonArgs;
        craneWithCommonArgs =
          x: craneLib.buildPackage (craneCommonArgs // { inherit cargoArtifacts; } // x);
        entraceApp = craneWithCommonArgs {
          pname = "entrace";
          cargoExtraArgs = "-p entrace_gui";
          postFixup = ''
            ENTRACE_BIN="$out/bin/entrace"
            patchelf --add-rpath ${pkgs.lib.makeLibraryPath rpath-libs} "$ENTRACE_BIN"
            patchelf \
              --add-needed libwayland-client.so \
              --add-needed libxkbcommon.so \
              --add-needed libEGL.so \
              --add-needed libluajit-5.1.so "$ENTRACE_BIN"
          '';
        };
      in
      {
        devShells.${system}.default = import ./shell.nix { inherit pkgs rustfmt-nightly; };
        packages.${system} = {
          default = entraceApp;
          entrace = entraceApp;
          entrace_core = craneWithCommonArgs {
            pname = "entrace_core";
            cargoExtraArgs = "-p entrace_core";
          };
          entrace_core_lite = craneWithCommonArgs {
            pname = "entrace_core_lite";
            cargoExtraArgs = "-p entrace_core --no-default-features";
          };
          entrace_script = craneWithCommonArgs {
            pname = "entrace-script";
            cargoExtraArgs = "-p entrace_script";
          };
        };
      }
    );
}
