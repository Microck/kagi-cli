{
  lib,
  rustPlatform,
  installShellFiles,
  stdenv,
}:

let
  cargoToml = lib.importTOML ../Cargo.toml;
in
rustPlatform.buildRustPackage {
  pname = cargoToml.package.name;
  version = cargoToml.package.version;

  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;

  # reqwest uses rustls-tls, so the package does not need OpenSSL or pkg-config.
  nativeBuildInputs = [ installShellFiles ];

  # The test suite uses local mock servers and can run in the sandbox.
  doCheck = true;

  postInstall = lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    installShellCompletion --cmd kagi \
      --bash <($out/bin/kagi --generate-completion bash) \
      --zsh  <($out/bin/kagi --generate-completion zsh) \
      --fish <($out/bin/kagi --generate-completion fish)
  '';

  meta = {
    description = cargoToml.package.description;
    homepage = "https://github.com/Microck/kagi-cli";
    changelog = "https://github.com/Microck/kagi-cli/blob/main/CHANGELOG.md";
    license = lib.licenses.mit;
    mainProgram = "kagi";
    platforms = lib.platforms.unix;
  };
}
