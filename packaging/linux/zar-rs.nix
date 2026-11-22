{
  lib,
  rustPlatform,
  pkg-config,
  makeWrapper,
  wayland,
  libxkbcommon,
  libGL,
  xorg,
  zstd,
}:

rustPlatform.buildRustPackage {
  pname = "zar-rs";
  version = (builtins.fromTOML (builtins.readFile ../../Cargo.toml)).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      ../../Cargo.toml
      ../../Cargo.lock
      ../../zarlib
      ../../zarlib-cli
      ../../zarlib-gui
      ../../zcopy
      ../../zarmcp
      ../../zarmount
      ../../czarlib
      ../../cxxzarlib
      ../../pyzarlib/Cargo.toml
      ../../pyzarlib/src
      ../../wasizarlib/Cargo.toml
      ../../wasizarlib/src
      ./zarg-mime.xml
    ];
  };
  cargoLock.lockFile = ../../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];
  buildInputs = [
    wayland
    xorg.libxcb
    zstd
  ];

  cargoBuildFlags = [
    "--locked"
    "--bins"
    "-p"
    "zarlib-cli"
    "-p"
    "zarlib-gui"
    "-p"
    "zarlib-zcopy"
    "-p"
    "zarmcp"
    "-p"
    "zarmount"
  ];

  # Workspace tests require additional fixtures and, for some tests, a display or FUSE.
  doCheck = false;
  doInstallCheck = true;

  postInstall = ''
    for binary in zarc zarg zcopy zarmcp zarmount; do
      test -x "$out/bin/$binary"
    done
    install -Dm644 zarlib-gui/data/zarg.desktop "$out/share/applications/zarg.desktop"
    install -Dm644 zarlib-gui/data/zarg-error.desktop "$out/share/applications/zarg-error.desktop"
    install -Dm644 zarlib-gui/assets/zarg-icon.png "$out/share/icons/hicolor/256x256/apps/zarg.png"
    install -Dm644 zarlib-gui/assets/zarg-error.png "$out/share/icons/hicolor/64x64/apps/zarg-error.png"
    install -Dm644 zarlib-gui/assets/application-x-zar.png "$out/share/icons/hicolor/64x64/mimetypes/application-x-zar.png"
    install -Dm644 packaging/linux/zarg-mime.xml "$out/share/mime/packages/zar-rs.xml"

    wrapProgram "$out/bin/zarg" --prefix LD_LIBRARY_PATH : ${
      lib.makeLibraryPath [
        libGL
        libxkbcommon
        wayland
        xorg.libX11
        xorg.libXcursor
        xorg.libXi
        xorg.libXrandr
        xorg.libxcb
      ]
    }
  '';

  installCheckPhase = ''
    runHook preInstallCheck
    "$out/bin/zarc" --version
    "$out/bin/zcopy" --version
    "$out/bin/zarmcp" --help > /dev/null
    "$out/bin/zarmount" --version
    runHook postInstallCheck
  '';

  meta = {
    description = "ZAR archive tools, graphical manager, MCP server and FUSE filesystem";
    homepage = "https://github.com/brmmm3/zar-rs";
    license = with lib.licenses; [ mit asl20 ];
    platforms = lib.platforms.linux;
    mainProgram = "zarc";
  };
}
