#define MyAppName "Tarvos"
#define MyAppVersion "1.5.0"
#define MyAppPublisher "Repo-Tech"
#define MyAppExeName "tarvos.exe"

[Setup]
AppId={{8E6B3D86-9F54-4B74-9A5E-2B2B7D7A15A1}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={userappdata}\Tarvos\bin
DefaultGroupName=Tarvos
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=output
OutputBaseFilename=Tarvos-Setup-Windows-x86_64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
Uninstallable=yes
ArchitecturesInstallIn64BitMode=x64

[Files]
Source: "payload\tarvos.exe"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{app};{olddata}"; Flags: preservestringtype

[Icons]
Name: "{group}\Tarvos"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"

[Run]
Filename: "{app}\{#MyAppExeName}"; Parameters: "--version"; Description: "Verify Tarvos installation"; Flags: postinstall skipifsilent
