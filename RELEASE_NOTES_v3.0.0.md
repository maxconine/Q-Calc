## Q Calc 3.0 for Windows: rebuilt as a native app

Q Calc on Windows is now a native Windows app, written from scratch in Rust on Win32, Direct2D and DirectWrite. There's no web view inside any more, so it's faster, much lighter, and runs on more PCs. It's the same calculator, with the same shortcuts and the same answers.

**If you already have Q Calc on Windows, you don't need to do anything.** Within a day it updates itself to 3.0 in the background, the same quiet way as every update: while the calculator is put away and you're not using the PC. Your settings, keybinds, history and saved rates come with it. Q Calc stays in the same folder, in the same Start menu entry and in **Installed apps**, and keeps starting when you sign in if it did before.

### What's new

- **Faster.** Answers are drawn in about a millisecond after each key, and Q Calc is ready within a few hundredths of a second of starting.
- **Lighter.** Hidden in the tray it uses under 10 MB of memory. The old app carried a whole browser engine (WebView2).
- **Messages, clips and files.** Type `message <name>` to send someone a message, and `messages` to see your conversations. You can also send what's on your clipboard, or drop files on the bar. Clips and files are end-to-end encrypted: they're sealed on your PC for the other person's key, and the relay only ever sees ciphertext. Text messages aren't end-to-end encrypted yet: they travel over HTTPS, and the relay passes them on without keeping them. If someone's key changes, Q Calc tells you and asks before trusting the new one, and **ⓘ** shows a safety code you can compare. Settings › Messages picks where saved files go and whether files save without asking.
- **Notifications.** A message, request or file that arrives while the bar is hidden shows as a small note in the corner. The note never takes focus, and it stays quiet in Do Not Disturb and in full-screen apps. Turn it off in Settings › Messages › Notifications.
- **More Windows.** Q Calc runs on every Windows 10 from the first release (1507, build 10240) and on Windows 11, on x64 and Arm PCs. Nothing else is needed: no WebView2 and no Visual C++ runtime.
- **Crash reports you control.** If Q Calc ever crashes, it saves a short report on your PC. The next time it opens, it offers to start a GitHub issue with the version and a one-line summary. Nothing is sent unless you choose to send it.

### Downloads

| File | For |
| --- | --- |
| `Q-Calc-3.0.0-x64-native-setup.exe` | most Windows PCs (Intel and AMD) |
| `Q-Calc-3.0.0-arm64-native-setup.exe` | Arm PCs (Snapdragon) |

Run the installer. It installs for your account only, so there's no admin prompt. If you already have Q Calc, it replaces it in place.

### Known issues

- **The installer isn't code-signed yet.** When you install it by hand, Windows SmartScreen may say *Windows protected your PC*: click **More info → Run anyway**. Automatic updates never show this.
- **Smart App Control.** New Windows 11 PCs often have *Smart App Control* on, and it quietly blocks apps that aren't code-signed yet, including Q Calc. You may see a brief notice that it blocked an app, or nothing at all. To check, open Windows Security → App & browser control → Smart App Control. If it's on, the only way to run Q Calc is to turn it off there, and Windows won't let you turn it back on without resetting the PC. If Smart App Control is off and Q Calc still doesn't start, look in `%LOCALAPPDATA%\com.maxconine.qcalc\qcalc.log`. A line for each start means Q Calc ran (please send us the file). No file means Windows or an antivirus stopped it before it started.
- If **Alt + Space** opens something else, such as PowerToys Run, choose another key under Keybinds in settings.

Found a problem? Use the [feedback form](https://forms.gle/acunzzzwuMZUmMiF9) or [open an issue](https://github.com/maxconine/Q-Calc/issues).
