# Rust library that exposes Python (with uv, ty, and ruff) interface via PyO3, with each language ecosystem in its own Nix profile
{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:
{
  # devenv.sh/languages/
  # The base library is written in Rust, should be available on all profiles
  languages = {
    nix = {
      enable = true;
      lsp.package = pkgs.nil;
    };
    rust = {
      enable = true;
      channel = "stable";
      # .rustfmt.toml needs nightly; devenv.lock pins both toolchains.
      toolchain.rustfmt = (inputs.rust-overlay.lib.mkRustBin { } pkgs).nightly.latest.rustfmt;
    };
  };

  packages = [
    pkgs.cargo-nextest
    pkgs.coreutils
    pkgs.curl
    pkgs.pkg-config
  ];

  env.SAKAI_VMTEST_ROOTFS =
    "docker.io/library/rust:${config.languages.rust.toolchainPackage.version}-bookworm";

  scripts.vmtest.exec = ''cargo xtask vmtest "$@"'';

  # Run together with `devenv test`, or individually with `devenv tasks run check:fmt`.
  tasks = {
    # Attach checks only in test mode, keeping shell entry and Zed formatting fast.
    "devenv:enterTest".after = lib.optionals config.devenv.isTesting [
      "check:fmt"
      "check:clippy"
      "check:test"
      "check:doc"
    ];
    "check:fmt" = {
      exec = "cargo fmt --all -- --check";
    };
    "check:clippy" = {
      exec = "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings";
    };
    "check:test" = {
      exec = "cargo nextest run --workspace --all-targets --all-features --locked";
    };
    "check:doc" = {
      # nextest does not execute documentation tests.
      exec = "cargo test --workspace --all-features --doc --locked";
    };
  };

  # devenv.sh/profiles/
  # namespaced profiles keep the Rust and Python toolchains isolated
  profiles = {
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
