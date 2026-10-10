{
  lib,
  stdenvNoCC,
  fetchurl,
  unzip,
  makeWrapper,
  autoPatchelfHook,
  gcc,
  libsecret,
  git,
}:
let
  # Checksums from https://cli.coderabbit.ai/releases/0.9.0/SHA256SUMS.
  sources = {
    aarch64-darwin = {
      platform = "darwin-arm64";
      sha256 = "f5f20fb7318ac99b6ffdf306abe3c82b80ed2382116e50163605a569b59583df";
    };
    aarch64-linux = {
      platform = "linux-arm64";
      sha256 = "ad8f700742981beb8759160dbbd786183af3d76c97d1595e3d64099e4f13c924";
    };
    x86_64-linux = {
      platform = "linux-x64";
      sha256 = "56e4c99de9d11106c3e6bf31034864db8f746cb97c9eaf9470adcfa6461da66f";
    };
  };
  source = sources.${stdenvNoCC.hostPlatform.system};
in
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "coderabbit-cli";
  version = "0.9.0";

  src = fetchurl {
    url = "https://cli.coderabbit.ai/releases/${finalAttrs.version}/coderabbit-${source.platform}.zip";
    inherit (source) sha256;
  };

  nativeBuildInputs = [ unzip makeWrapper ] ++ lib.optionals stdenvNoCC.hostPlatform.isLinux [ autoPatchelfHook ];
  buildInputs = lib.optionals stdenvNoCC.hostPlatform.isLinux [ gcc.cc.lib ];

  sourceRoot = ".";
  # Stripping can corrupt the embedded Bun runtime.
  dontStrip = true;
  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 coderabbit "$out/bin/coderabbit"
    wrapProgram "$out/bin/coderabbit" \
      --set CODERABBIT_CLI_DISABLE_AUTO_UPDATE true \
      --prefix PATH : ${lib.makeBinPath [ git ]} \
      ${lib.optionalString stdenvNoCC.hostPlatform.isLinux "--prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ libsecret ]}"}
    ln -s coderabbit "$out/bin/cr"
    runHook postInstall
  '';

  meta = {
    description = "AI-powered code review CLI";
    homepage = "https://docs.coderabbit.ai/cli";
    license = lib.licenses.unfree;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
    platforms = builtins.attrNames sources;
    mainProgram = "coderabbit";
  };
})
