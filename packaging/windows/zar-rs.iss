; Inno Setup script for the ZAR command-line tool (zarc) and archive manager (zarg).
;
; Build (from the repository root, after `cargo build --release --bins`):
;   iscc /DAppVersion=0.7.0 packaging\windows\zar-rs.iss
; Options:
;   /DSourceDir=<dir>   directory containing zarc.exe and zarg.exe
;                       (default: target\release; use target\<triple>\release
;                       when building with --target)
;   /DArch=arm64        build the installer for Windows on ARM (default: x64)

#ifndef AppVersion
  #define AppVersion "0.7.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif
#ifndef Arch
  #define Arch "x64"
#endif

#define AppName "ZAR"
#define AppPublisher "The ZAR contributors"
#define AppURL "https://github.com/brmmm3/zar-rs"

[Setup]
AppId={{6F0B7A5C-3E57-4D5E-9B0A-5A1D6E2F7C11}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
DefaultDirName={autopf}\ZAR
DefaultGroupName=ZAR
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE-MIT
OutputDir=..\..\dist
OutputBaseFilename=zar-rs-{#AppVersion}-windows-{#Arch}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName} {#AppVersion}
UninstallDisplayIcon={app}\zarg.exe
ChangesEnvironment=yes
PrivilegesRequired=admin
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "addtopath"; Description: "Add the &zarc command-line tool to the system PATH"; GroupDescription: "Command line:"
Name: "desktopicon"; Description: "Create a &desktop shortcut for the archive manager"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\zarc.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\zarg.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\ReadMe.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\zarc\ReadMe.md"; DestDir: "{app}"; DestName: "zarc-ReadMe.md"; Flags: ignoreversion
Source: "..\..\zarg\ReadMe.md"; DestDir: "{app}"; DestName: "zarg-ReadMe.md"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\ZAR Archive Manager"; Filename: "{app}\zarg.exe"
Name: "{autodesktop}\ZAR Archive Manager"; Filename: "{app}\zarg.exe"; Tasks: desktopicon

[Registry]
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; \
  ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; \
  Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}'))

[Run]
Filename: "{app}\zarg.exe"; Description: "Launch ZAR Archive Manager"; Flags: nowait postinstall skipifsilent unchecked

[Code]
const
  EnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';

function NeedsAddPath(Param: string): Boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE, EnvironmentKey, 'Path', OrigPath) then
  begin
    Result := True;
    Exit;
  end;
  Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

procedure RemoveFromPath(const Dir: string);
var
  OrigPath, Needle, Padded: string;
  Position: Integer;
begin
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE, EnvironmentKey, 'Path', OrigPath) then
    Exit;
  Needle := ';' + Uppercase(Dir) + ';';
  Padded := ';' + OrigPath + ';';
  Position := Pos(Needle, Uppercase(Padded));
  if Position = 0 then
    Exit;
  Delete(Padded, Position, Length(Needle) - 1);
  // Padded now starts and ends with a separator that was only added for matching.
  if (Length(Padded) > 0) and (Padded[1] = ';') then
    Delete(Padded, 1, 1);
  if (Length(Padded) > 0) and (Padded[Length(Padded)] = ';') then
    Delete(Padded, Length(Padded), 1);
  RegWriteExpandStringValue(HKEY_LOCAL_MACHINE, EnvironmentKey, 'Path', Padded);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}'));
end;
