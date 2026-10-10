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
      version = "1.99.0";
      # .rustfmt.toml needs nightly; devenv.lock pins both toolchains.
      toolchain.rustfmt = (inputs.rust-overlay.lib.mkRustBin { } pkgs).nightly.latest.rustfmt;
    };
  };

  packages = [
    inputs.llm-agents.packages.${pkgs.stdenv.hostPlatform.system}.coderabbit-cli
    pkgs.cargo-nextest
    pkgs.coreutils
    pkgs.curl
    pkgs.k3d
    pkgs.kubectl
    pkgs.pkg-config
    pkgs.protobuf
  ] ++ lib.optionals (pkgs.stdenv.hostPlatform.isLinux && pkgs.stdenv.hostPlatform.isx86_64) [
    pkgs.qemu
    (pkgs.runCommand "vmtest" { } ''
      install -Dm755 ${inputs.vmtest.outPath} $out/bin/vmtest-upstream
    '')
  ];

  scripts.vmtest.exec = ''cargo xtask vmtest "$@"'';

  # Container tooling follows the resolved devenv toolchain and input URL.
  env.SAKAI_VMTEST_RUST_VERSION = config.languages.rust.toolchain.rustc.version;
  env.SAKAI_VMTEST_URL = (builtins.fromJSON (builtins.readFile ./devenv.lock)).nodes.vmtest.locked.url;
  # Nix manages the CodeRabbit version through devenv.lock.
  env.CODERABBIT_CLI_DISABLE_AUTO_UPDATE = "true";

  # Run together with `devenv test`, or individually with `devenv tasks run check:fmt`.
  tasks = {
    # Attach checks only in test mode, keeping shell entry and Zed formatting fast.
    "devenv:enterTest".after = lib.optionals config.devenv.isTesting [
      "check:fmt"
      "check:clippy"
      "check:test"
      "check:doc"
    ];
    "check:fmt".exec = "cargo fmt --all -- --check";
    "check:clippy".exec = "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings";
    "check:test".exec = "cargo nextest run --workspace --all-targets --all-features --locked";
    # nextest does not execute documentation tests.
    "check:doc".exec = "cargo test --workspace --all-features --doc --locked";
    # Explicit opt-in: this integration check requires a running Docker daemon.
    "check:kubernetes".exec = ''
      set -euo pipefail
      k3d cluster create --config tools/kubernetes/cluster.yaml
      trap 'k3d cluster delete sakai-resource-test' EXIT
      kubeconfig="$(k3d kubeconfig write sakai-resource-test)"
      export KUBECONFIG="$kubeconfig"
      docker build -f tools/kubernetes/Containerfile -t sakai-resource-test:local .
      k3d image import --cluster sakai-resource-test sakai-resource-test:local
      kubectl create -f tools/kubernetes/pod.yaml
      status=0
      kubectl wait --for=jsonpath='{.status.phase}'=Succeeded pod/sakai-resource-test --timeout=120s || status=$?
      if [ "$status" -ne 0 ]; then kubectl describe pod sakai-resource-test; fi
      kubectl logs sakai-resource-test
      kubectl get pod sakai-resource-test -o wide
      exit "$status"
    '';
  };

  # devenv.sh/profiles/
  # namespaced profiles keep the Rust and Python toolchains isolated
  profiles = {
    python.module = { config, pkgs, ... }: {
      languages.python = {
        enable = true;
        package = pkgs.python315;
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

      tasks."devenv:enterTest".after = lib.optionals config.devenv.isTesting [
        "check:ruff"
        "check:ruff-format"
        "check:ty"
      ];
      tasks."check:ruff".exec = "ruff check .";
      tasks."check:ruff-format".exec = "ruff format --check .";
      tasks."check:ty".exec = "ty check --project sakai-python .";
    };
  };
  # See full reference at https://devenv.sh/reference/options/
}
