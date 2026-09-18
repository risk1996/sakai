# Rust library that exposes Python (with uv, ty, and ruff) interface via PyO3, with each language ecosystem in its own Nix profile
{ pkgs, lib, config, ... }:
{
  # devenv.sh/languages/
  # The base library is written in Rust, should be available on all profiles
  languages = {
    rust = {
      enable = true;
      channel = "stable";
    };
  };

  packages = [
    pkgs.pkg-config
  ];

  # devenv.sh/profiles/
  # namespaced profiles keep the Rust and Python toolchains isolated
  profiles = {
    rust.module = {
      languages.rust.enable = true;
    };

    python.module = {
      languages.python = {
        enable = true;
        uv.enable = true;
      };

      packages = [
        pkgs.uv
        pkgs.ty
        pkgs.ruff
        pkgs.maturin
      ];

      # PyO3 requires the shared libpython to resolve at link/runtime.
      env.PYO3_PYTHON = "${config.languages.python.package}/bin/python3";
      env.LD_LIBRARY_PATH = lib.makeLibraryPath [ config.languages.python.package ];
    };
  };
  # See full reference at https://devenv.sh/reference/options/
}
