# Packaging

Installers for `zarc` (CLI) and `zarg` (GUI). Output goes to `dist/`.

## Windows (Inno Setup)

```powershell
cargo build --release --bins
iscc /DAppVersion=0.6.0 packaging\windows\zar-rs.iss
```

Use `/DSourceDir=target\<triple>\release` when building with `--target`, and
`/DArch=arm64` for Windows on ARM. The installer can add `zarc` to the system
`PATH` and create Start menu and desktop shortcuts.

## Linux

```sh
packaging/linux/build-packages.sh [deb|rpm|appimage|flatpak|all]
```

| Format   | Tool needed                         | Result                          |
|----------|-------------------------------------|---------------------------------|
| deb      | `dpkg-deb`                          | `zar-rs_<version>_<arch>.deb`   |
| rpm      | `rpmbuild`                          | `zar-rs-<version>-1.<arch>.rpm` |
| appimage | `appimagetool` (or `APPIMAGETOOL`)  | `ZAR-<version>-<arch>.AppImage` |
| flatpak  | `flatpak-builder`, `flatpak`        | `zar-rs-<version>-<arch>.flatpak` |

deb, rpm and AppImage are assembled from the release binaries
(`SKIP_BUILD=1` reuses existing ones; `BIN_DIR` selects another directory).
The AppImage starts `zarg`; invoked through a symlink named `zarc` (or with
`ZAR_CLI=1`) it starts the CLI.

The Flatpak is built from source inside the Freedesktop SDK and downloads
crates during the build, so it is suitable for local bundles but not Flathub.
Its desktop file is named after the app ID, while `zarg` sets the window app ID
`zarg`, so some Wayland shells may not show the icon for the Flatpak window.
