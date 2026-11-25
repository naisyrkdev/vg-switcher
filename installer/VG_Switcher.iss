#define MyAppName "VG Switcher"
#ifndef MyAppVersion
  #define MyAppVersion "0.5.0"
#endif
#define MyAppPublisher "VG Switcher"
#define MyAppExeName "VG_Switcher.exe"

[Setup]
AppId={{B6C2B8A4-6C2E-4C7F-9A6B-7F8E8C1D4E21}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\VG Switcher
DefaultGroupName=VG Switcher
UninstallDisplayIcon={app}\{#MyAppExeName}
OutputDir=Output
OutputBaseFilename=VG_Switcher_Setup
SetupIconFile=..\assets\icon.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "..\target\release\VG_Switcher.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\VG Switcher"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall VG Switcher"; Filename: "{uninstallexe}"
Name: "{autodesktop}\VG Switcher"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch VG Switcher"; Flags: nowait postinstall skipifsilent

[Code]
procedure KillRunningApp;
var
  ResultCode: Integer;
begin
  Exec('taskkill.exe', '/F /IM {#MyAppExeName} /T', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure RemoveAutostartTask;
var
  ResultCode: Integer;
begin
  Exec('schtasks.exe', '/delete /tn VG_Switcher /f', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    KillRunningApp;
end;

function InitializeUninstall(): Boolean;
begin
  KillRunningApp;
  RemoveAutostartTask;
  Result := True;
end;
