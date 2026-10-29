## Q Calc 3.0.1: the native app comes to the Mac

Q Calc on the Mac is now a native app, written in Swift and Rust, like the new Windows app. There's no web view inside any more, so it opens instantly and uses far less memory. It's the same calculator, with the same shortcuts and the same answers.

**If you already have Q Calc on your Mac, you don't need to do anything.** It updates itself to 3.0.1 the usual way, and your settings, keybinds and saved rates come with it.

### What's new on the Mac

- **Faster.** The bar shows and hides with no fade, and answers keep up with every key.
- **Messages, clips and files.** Type `message <name>` to send someone a message, and `messages` to see your conversations. You can also send what's on your clipboard, or drop files on the bar. Clips and files are end-to-end encrypted. Settings › Messages picks where saved files go and whether files save without asking.
- **Notes in the corner.** A message, request or file that arrives while the bar is hidden shows as a small Q Calc note in the corner of the screen, in place of a macOS notification. It never takes focus, and a click opens the conversation. It stays quiet over full-screen apps. Turn it off, or hide the message text, in Settings › Messages.
- **Bring in highlighted text.** Turn this on in Settings and text you've highlighted in another app goes into the bar when you open it. It's off until you turn it on, and then macOS asks for Accessibility access.
- **Clean paste.** Pasted text becomes one plain line. Paste a sentence or a paragraph and Q Calc puts the plain-text version back on your clipboard, then opens fresh the next time.
- **Your colour for units and functions.** Settings › Units and functions picks green, dark or grey.

### Fixes on both Mac and Windows

- `2.03e-12/22` reads as `(2.03e-12)/22` in the typeset preview, as the answer always did.
- `ans in mm` gives no answer when the last answer has no unit, and in `12122 in in mm` the second `in` is read as "to".

### Downloads

| File | For |
| --- | --- |
| `Q-Calc-3.0.1-apple-silicon.zip` | Macs with Apple silicon (M1 and later) |
| `Q-Calc-3.0.1-intel.zip` | Intel Macs |
| `Q-Calc-3.0.1-x64-native-setup.exe` | most Windows PCs (Intel and AMD) |
| `Q-Calc-3.0.1-arm64-native-setup.exe` | Arm PCs (Snapdragon) |

On the Mac, unzip and drag **Q Calc** to Applications. It runs on macOS 12 and later.

### Known issues

- **The Mac app isn't notarized yet.** Opened by hand the first time, macOS may say it can't check it for malware: right-click **Q Calc** › **Open**, or allow it in System Settings › Privacy & Security. Automatic updates never show this.
- **The Windows installer isn't code-signed yet.** Windows SmartScreen may say *Windows protected your PC*: click **More info → Run anyway**. Automatic updates never show this.

Found a problem? Use the [feedback form](https://forms.gle/acunzzzwuMZUmMiF9) or [open an issue](https://github.com/maxconine/Q-Calc/issues).
