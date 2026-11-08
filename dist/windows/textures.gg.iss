; The textures.gg installer for Windows (Inno Setup 6). scripts/package-windows.sh
; runs it with /DVersion=<version> /DSource=<the staged folder> /DOutput=<folder>.
; It installs for the current user only (no administrator prompt), into
; %LOCALAPPDATA%\Programs\textures.gg, with a Start menu entry and an
; uninstaller; installing again updates it in place.

#ifndef Version
  #error Pass /DVersion=<version>
#endif

[Setup]
; Fixed for good: Windows knows the installed app by it, so updates replace it.
AppId={{6F1D0B2E-7C1A-4E54-9B8F-2A7E3D4C5B61}
AppName=textures.gg
AppVersion={#Version}
AppVerName=textures.gg {#Version}
AppPublisher=SaavyLab LLC
AppPublisherURL=https://textures.gg
AppSupportURL=https://textures.gg
DefaultDirName={localappdata}\Programs\textures.gg
DefaultGroupName=textures.gg
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
OutputDir={#Output}
OutputBaseFilename=textures.gg-{#Version}-windows-x86_64-setup
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\tgg-studio.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\textures.gg"; Filename: "{app}\tgg-studio.exe"

[Run]
Filename: "{app}\tgg-studio.exe"; Description: "Open textures.gg"; Flags: nowait postinstall skipifsilent
