; Inno Setup Skript fuer Bill2Mail
; Vorher die Release-Version bauen:  npm run tauri build -- --no-bundle
; Die Versionsnummer wird aus der .exe gelesen (kommt aus "version" in src-tauri/tauri.conf.json).

#define ApplicationName 'Bill2Mail'
#define ProjectRoot AddBackslash(SourcePath) + '..'
#define ExeName 'bill2mail.exe'
#define ExePath ProjectRoot + '\src-tauri\target\release\' + ExeName
#define VMajor
#define VMinor
#define VRev
#define VBuild
#expr ParseVersion(ExePath, VMajor, VMinor, VRev, VBuild)
#define ApplicationVersion Str(VMajor) + '.' + Str(VMinor) + '.' + Str(VRev)

[Setup]
;SignTool=signtool
; Neue, eigene AppId (nicht die vom alten pdf2mail), damit sich beide Programme nicht gegenseitig ueberschreiben.
AppId={{543D7085-1F5D-49B6-B684-59FDDCF16465}
AppName={#ApplicationName}
AppVersion={#ApplicationVersion}
AppVerName={#ApplicationName} {#ApplicationVersion}
AppPublisher=Alexander Predl
AppPublisherURL=https://www.predl.cc
AppSupportURL=https://www.predl.cc
AppUpdatesURL=https://www.predl.cc
AppCopyright=© 2026 Alexander Predl
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DefaultDirName={autopf}\{#ApplicationName}
DefaultGroupName={#ApplicationName}
Compression=lzma2
SolidCompression=yes
ShowLanguageDialog=no
; WebView2 (Voraussetzung von Tauri) gibt es erst ab Windows 10
MinVersion=10.0
SetupIconFile={#ProjectRoot}\src-tauri\icons\icon.ico
VersionInfoCompany=Alexander Predl
VersionInfoDescription={#ApplicationName} Setup
VersionInfoProductName={#ApplicationName}
VersionInfoTextVersion={#ApplicationVersion}
VersionInfoVersion={#ApplicationVersion}
OutputBaseFilename=bill2mail_setup_{#ApplicationVersion}
OutputDir=Output
DisableWelcomePage=no
DisableDirPage=no
UninstallDisplayIcon={app}\{#ExeName}
MissingRunOnceIdsWarning=no
CloseApplications=yes
RestartApplications=no

[Languages]
Name: de; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#ApplicationName}"; Filename: "{app}\{#ExeName}"; WorkingDir: "{app}"
Name: "{group}\{#ApplicationName} deinstallieren"; Filename: "{uninstallexe}"; IconFilename: "{uninstallexe}"
Name: "{autodesktop}\{#ApplicationName}"; Filename: "{app}\{#ExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#ExeName}"; Description: "{#ApplicationName} starten"; Flags: postinstall nowait skipifsilent

; Kundenliste, Vorlage und Einstellungen liegen in %APPDATA%\at.bill2mail.app und bleiben
; bei Deinstallation und Update absichtlich erhalten.

[Code]
function WebView2Vorhanden: Boolean;
var
  V: String;
begin
  Result :=
    RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', V) or
    RegQueryStringValue(HKLM, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', V) or
    RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', V);
  Result := Result and (V <> '') and (V <> '0.0.0.0');
end;

function InitializeSetup: Boolean;
begin
  Result := True;
  if not WebView2Vorhanden then
    Result := MsgBox(
      'Die Microsoft Edge WebView2 Runtime wurde nicht gefunden. Sie wird von {#ApplicationName} zur Anzeige benötigt ' +
      '(auf Windows 11 und aktuellen Windows-10-Systemen ist sie bereits enthalten).' + #13#10#13#10 +
      'Download: https://developer.microsoft.com/microsoft-edge/webview2/' + #13#10#13#10 +
      'Trotzdem installieren?', mbConfirmation, MB_YESNO) = IDYES;
end;
