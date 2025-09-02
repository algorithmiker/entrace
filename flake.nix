{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    fenix.url = "github:nix-community/fenix/monthly";
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
    }:
    let
      systems = nixpkgs.lib.systems.flakeExposed;
      forAllSystems = f: builtins.foldl' nixpkgs.lib.recursiveUpdate { } (builtins.map f systems);
    in
    forAllSystems (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustfmt_nightly = fenix.packages.${system}.default.rustfmt;
      in
      {
        devShells.${system}.default =
          let
            libPathPackages = [
              pkgs.wayland
              pkgs.libxkbcommon
              pkgs.libGL
            ];
            _libPathPackagesStr = builtins.toString (pkgs.lib.makeLibraryPath libPathPackages);
          in
          pkgs.mkShell {
            packages = libPathPackages ++ [
              pkgs.sqlite
              pkgs.mold-wrapped
              pkgs.pkg-config
              pkgs.luajit
              pkgs.zenity
              rustfmt_nightly
              pkgs.cargo-about
            ];
            env.RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
            shellHook = ''
              export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${_libPathPackagesStr}"
            '';
          };
        packages.${system}.default = pkgs.callPackage ./package.nix { };
      }
    );
}
