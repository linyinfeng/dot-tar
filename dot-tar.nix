{ rustPlatform, lib, pkg-config, openssl }:

rustPlatform.buildRustPackage
{
  pname = "dot-tar";
  version = "0.0.1";

  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [
    pkg-config
  ];
  buildInputs = [
    openssl
  ];

  meta = with lib; {
    license = licenses.mit;
  };
}
