; The Windows setup program: one installer carrying both programs this repository builds.
;
; Compiled by tools/platform/windows/installer.sh, which is the only thing that should invoke it --
; that script assembles the payload, reads the version out of a binary and passes the five defines
; below. Compiling this file by hand is possible and needs all five:
;
;   ISCC.exe /DPayload=C:\...\dist\setup\windows\payload /DVersion=1.7.0 ^
;            /DGenerated=C:\...\dist\setup\windows\generated ^
;            /DOutDir=C:\...\dist\setup\windows ^
;            /DOutBase=km-video-tools-setup-1.7.0-x86_64 tools\platform\windows\installer.iss
;
; **This file names every file it installs.** installer.sh reads the [Files] section back out and
; reconciles it against what is actually in the payload, so a file that arrives there and is not
; mentioned here fails the build rather than being silently left out of the setup. A carrier that
; quietly loses a file is the one failure it may not have.
;
; **The console twin is deliberately absent.** `km-video-downloader-console.exe` exists so that a
; Windows *folder* gives somebody something to type; an installed build has a Start Menu entry and a
; PATH instead, and a second executable differing from the first only in its subsystem is exactly the
; confusion an installer exists to remove. The portable folder `task dist` stages still carries it.
;
; **Per-user, because nothing here needs to be otherwise.** Two programs, no service, no driver and
; no shared state: `PrivilegesRequired=lowest` puts them in {localappdata}\Programs, adds the PATH
; entry under HKCU\Environment, and raises no UAC prompt at any point. It is also where `winget`
; already puts per-user software.

#ifndef Payload
  #error Payload is not defined. Run tools/platform/windows/installer.sh; it assembles the folder this needs.
#endif
#ifndef Version
  #error Version is not defined. installer.sh reads it out of the staged binary.
#endif
#ifndef Generated
  #error Generated is not defined. installer.sh writes the installed build's README there.
  #error It is not in the payload, because the payload's own README describes a folder.
#endif
#ifndef OutDir
  #error OutDir is not defined.
#endif
#ifndef OutBase
  #error OutBase is not defined.
#endif

#define AppName "KM Video Tools"
#define AppPublisher "Rangel Reale"
#define AppUrl "https://github.com/rrgmc/km-video-tools"

; The runtime km-video-downloader puts in its window. `wry` uses the platform's own webview, which on
; Windows is WebView2 -- Microsoft's evergreen bootstrapper is tiny and installs per-user without
; elevation. The GUID is the WebView2 runtime's own registration under EdgeUpdate: a Microsoft
; published constant, not something to regenerate.
#define WebView2Url "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
#define WebView2Guid "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

[Setup]
; **Never change AppId.** It is what makes a second run an upgrade of the first rather than a second
; copy, and what lets the uninstaller be found. The version moves; this does not.
AppId={{6F2A4C18-93D7-4E5B-9A21-7C0E8B4D51F3}
AppName={#AppName}
AppVersion={#Version}
AppVerName={#AppName} {#Version}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}
VersionInfoVersion={#Version}

; Per-user. No UAC prompt, ever -- `lowest` makes {autopf} resolve to {localappdata}\Programs and
; {group} to this user's own Start Menu.
PrivilegesRequired=lowest
DefaultDirName={autopf}\KM Video Tools
DefaultGroupName={#AppName}
AllowNoIcons=yes

ArchitecturesAllowed=x64compatible
MinVersion=10.0

OutputDir={#OutDir}
OutputBaseFilename={#OutBase}
; Three levels up: this file lives at tools/platform/windows/, so `..\..\..` is the repository root.
SetupIconFile={#SourcePath}\..\..\..\icon\km-video-downloader.ico
UninstallDisplayIcon={app}\km-video-downloader.exe

; Two small executables and four text files. lzma2/max costs a second here and saves little worth
; measuring, but a setup program is a thing people download and the default is worse for no reason.
Compression=lzma2/max
SolidCompression=yes

; A declaration to Windows rather than behavior: it makes Inno broadcast WM_SETTINGCHANGE after the
; PATH edit in [Code], so a console opened afterwards sees it without a sign-out.
ChangesEnvironment=yes

WizardStyle=modern
DisableWelcomePage=no
DisableDirPage=no
DisableProgramGroupPage=no

; Inno finds files in use through the Restart Manager and offers to close them, which is what stops a
; mid-install failure naming one locked executable. Neither program registers a named mutex, so this
; is the mechanism that applies.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Types]
Name: "full"; Description: "Both programs"
Name: "custom"; Description: "Choose what to install"; Flags: iscustom

[Components]
Name: "downloader"; Description: "KM Video Downloader -- fetch videos in a window"; Types: full; Flags: checkablealone
Name: "fetch"; Description: "km-video-fetch -- the same fetch on the command line"; Types: full

[Tasks]
; Ticked by default: km-video-fetch is a command line and is useless if it cannot be typed, and the
; windowed program is harmless on a PATH. Removal on uninstall is in [Code] -- Inno has no
; declarative form for taking one entry back out of a value it did not create.
Name: "addpath"; Description: "Add the installation folder to my PATH"; GroupDescription: "Set up:"
Name: "desktopicon"; Description: "Create a desktop shortcut for KM Video Downloader"; \
  GroupDescription: "Set up:"; Components: downloader; Flags: unchecked

[Files]
; -- the programs. One per component; no console twin, see the header. ---------------------------
Source: "{#Payload}\km-video-downloader.exe"; DestDir: "{app}"; Components: downloader; Flags: ignoreversion
Source: "{#Payload}\km-video-fetch.exe";      DestDir: "{app}"; Components: fetch;      Flags: ignoreversion

; -- the per-program documents, renamed by installer.sh so both fit in one folder -----------------
;
; No `Components:` on the licences: they are three kilobytes, and both licences cover both programs
; whichever one was chosen. The READMEs do carry one, because a folder holding instructions for a
; program that was not installed is worse than a folder holding one document.
Source: "{#Payload}\README-km-video-downloader.txt"; DestDir: "{app}"; Components: downloader; Flags: ignoreversion
Source: "{#Payload}\README-km-video-fetch.txt";      DestDir: "{app}"; Components: fetch;      Flags: ignoreversion
Source: "{#Payload}\LICENSE-MIT";                    DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\LICENSE-APACHE";                 DestDir: "{app}"; Flags: ignoreversion

; -- the document an installed build gets ----------------------------------------------------------
;
; **Not the payload's own README.txt**, which is deliberately never copied into the payload at all:
; it describes a folder somebody unpacked and is wrong about an installed build in most of its
; sentences -- there is no folder to keep, removal is Add or remove programs, and the command is on
; the PATH rather than in the current directory. `dist_installed_readme` writes this one.
Source: "{#Generated}\README.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\KM Video Downloader";       Filename: "{app}\km-video-downloader.exe"; Components: downloader
Name: "{group}\Read me first";             Filename: "{app}\README.txt"
Name: "{group}\Uninstall {#AppName}";      Filename: "{uninstallexe}"
Name: "{autodesktop}\KM Video Downloader"; Filename: "{app}\km-video-downloader.exe"; \
  Components: downloader; Tasks: desktopicon

[Run]
; The WebView2 bootstrapper, when [Code] decided one was needed and managed to fetch it. Per-user and
; silent, so this raises no prompt of its own.
Filename: "{tmp}\MicrosoftEdgeWebview2Setup.exe"; Parameters: "/silent /install"; \
  Check: WebView2WasFetched; StatusMsg: "Installing the Microsoft WebView2 runtime..."; Flags: runhidden

Filename: "{app}\km-video-downloader.exe"; Description: "Start KM Video Downloader now"; \
  Components: downloader; Flags: nowait postinstall skipifsilent unchecked

[Code]

const
  EnvironmentKey = 'Environment';

var
  DownloadPage: TDownloadWizardPage;
  WebView2Fetched: Boolean;

{ ---- WebView2 -------------------------------------------------------------------------------- }

{ The runtime registers a `pv` under EdgeUpdate. Three places are checked because Setup is a 32-bit
  process: HKLM reads are redirected into WOW6432Node, so the explicit 64-bit view is asked as well,
  and a per-user runtime lives under HKCU. `0.0.0.0` is what a stale registration left by an
  uninstall looks like, and it means the runtime is not there. }
function WebView2Installed: Boolean;
var
  Version: String;
  Path: String;
begin
  Path := 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{#WebView2Guid}';
  Result :=
    (RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{#WebView2Guid}', 'pv', Version)
      and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKLM, Path, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKCU, Path, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0'));
end;

{ Only the windowed program cares. An install of km-video-fetch alone must not reach the network --
  a command line has no window and no webview in it. }
function NeedsWebView2: Boolean;
begin
  Result := WizardIsComponentSelected('downloader') and not WebView2Installed;
end;

function WebView2WasFetched: Boolean;
begin
  Result := WebView2Fetched;
end;

function OnDownloadProgress(const Url, Filename: String; const Progress, ProgressMax: Int64): Boolean;
begin
  Result := True;
end;

{ ---- PATH ------------------------------------------------------------------------------------ }

{ Both directions live here rather than half in [Registry], so that the add and the remove cannot
  drift apart -- and because taking one entry back out of a value this installer did not create has
  no declarative form at all.

  Read with RegQueryStringValue and written back with RegWriteExpandStringValue: the user's Path
  routinely contains %USERPROFILE% and friends, and rewriting it as a plain string would expand them
  permanently. Inno's Pascal strings have no length limit, so a long real-world Path survives. }

function PathContains(const Haystack, Needle: String): Boolean;
begin
  Result := Pos(';' + Uppercase(Needle) + ';', ';' + Uppercase(Haystack) + ';') > 0;
end;

procedure AddToPath;
var
  Existing: String;
begin
  if not RegQueryStringValue(HKCU, EnvironmentKey, 'Path', Existing) then
    Existing := '';
  if PathContains(Existing, ExpandConstant('{app}')) then
    exit;
  if Existing = '' then
    Existing := ExpandConstant('{app}')
  else if Copy(Existing, Length(Existing), 1) = ';' then
    Existing := Existing + ExpandConstant('{app}')
  else
    Existing := Existing + ';' + ExpandConstant('{app}');
  RegWriteExpandStringValue(HKCU, EnvironmentKey, 'Path', Existing);
end;

procedure RemoveFromPath;
var
  Existing, Rebuilt, Part: String;
  P: Integer;
  Target: String;
begin
  if not RegQueryStringValue(HKCU, EnvironmentKey, 'Path', Existing) then
    exit;
  Target := Uppercase(ExpandConstant('{app}'));
  Rebuilt := '';
  { Split on ';' and keep every entry that is not ours. Only the entry this installer added is
    dropped, matched whole -- a substring match would take a sibling folder with a longer name. }
  while Existing <> '' do
  begin
    P := Pos(';', Existing);
    if P = 0 then
    begin
      Part := Existing;
      Existing := '';
    end
    else
    begin
      Part := Copy(Existing, 1, P - 1);
      Existing := Copy(Existing, P + 1, Length(Existing));
    end;
    if (Part <> '') and (Uppercase(Part) <> Target) then
    begin
      if Rebuilt = '' then
        Rebuilt := Part
      else
        Rebuilt := Rebuilt + ';' + Part;
    end;
  end;
  RegWriteExpandStringValue(HKCU, EnvironmentKey, 'Path', Rebuilt);
end;

{ ---- wiring ---------------------------------------------------------------------------------- }

procedure InitializeWizard;
begin
  WebView2Fetched := False;
  DownloadPage := CreateDownloadPage(
    'Downloading the WebView2 runtime',
    'KM Video Downloader needs Microsoft''s WebView2 runtime, which is not on this computer.',
    @OnDownloadProgress);
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID <> wpReady) or not NeedsWebView2 then
    exit;

  DownloadPage.Clear;
  DownloadPage.Add('{#WebView2Url}', 'MicrosoftEdgeWebview2Setup.exe', '');
  DownloadPage.Show;
  try
    try
      DownloadPage.Download;
      WebView2Fetched := True;
    except
      { **Not a failed install.** Say what did not happen, name the URL and carry on: the runtime can
        be installed afterwards, and stopping here would leave nothing installed at all. }
      MsgBox('The WebView2 runtime could not be downloaded:' + #13#10#13#10 +
             GetExceptionMessage + #13#10#13#10 +
             'Setup will carry on, but KM Video Downloader cannot open its window until the ' +
             'runtime is installed.' + #13#10#13#10 +
             'To fix it later, install the runtime from:' + #13#10 + '{#WebView2Url}',
             mbInformation, MB_OK);
    end;
  finally
    DownloadPage.Hide;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addpath') then
    AddToPath;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveFromPath;

  { Said once, at the end, because the thing people fear about an uninstaller is that it takes the
    downloads with it. It does not, and neither the videos nor the settings are guessable. }
  if (CurUninstallStep = usPostUninstall) and not UninstallSilent then
    MsgBox('{#AppName} has been removed.' + #13#10#13#10 +
           'The videos you downloaded have been left alone, wherever you put them, and so has the ' +
           'folder you chose. Your settings are under:' + #13#10#13#10 +
           ExpandConstant('{userappdata}\km-video-downloader') + #13#10#13#10 +
           'Delete that folder by hand if you want it gone.',
           mbInformation, MB_OK);
end;
