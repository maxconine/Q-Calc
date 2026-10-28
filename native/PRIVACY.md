# Privacy of messages, clips and files

This covers what the native Mac app sends through Q Calc's relay (`relay/`). Messages, clips and files go only between messaging names.

## What's end-to-end encrypted

- **Files.** The file itself, its name, its type, its size, its SHA-256 and the key it's encrypted with.
- **Clipboard shares (clips).** The text.

Only the recipient's Mac can open these. The relay passes them on but can't read them, and nor can anyone who reads the relay's storage.

## What isn't

- **Text messages** (`message max hi`) aren't end-to-end encrypted. They go to the relay over HTTPS, and the relay passes them on as they are. That keeps them readable by older copies of Q Calc and by the Windows and browser apps. The relay doesn't keep them.
- **Who and when.** The relay sees who sent a file or clip to whom, when, and how long the encrypted file is. It has to, to deliver it and to keep the size limits.
- **Names and public keys.** A messaging name and its public key are public. Anyone with a name can look them up.

## How it works

- **Keys.** The first time it's needed, each Mac makes an X25519 key pair. The private key stays in the login Keychain, marked for this device only. It's never synced, exported or sent. The public key is published with the name, using the name's own relay key, so only the name's holder can set it. A test build (Q Calc Dev) keeps its own key, apart from Q Calc's, in a file in its own support folder (`~/Library/Application Support/Q Calc Dev/share-key`) that only your user account can read, not in the Keychain. Test builds aren't signed with a developer certificate, so the Keychain treated every new build as a different app and asked for your login password each time. A file is less protected than the Keychain: any app running as you could read it.
- **Sealing.** For each file or clip the sender draws a random 128-bit id and a fresh ephemeral X25519 key. HKDF-SHA256 makes a one-time key from two agreements: the ephemeral key with the recipient's key, and the sender's key with the recipient's. The HKDF info is the scheme's version, the kind (`file` or `clip`), both names, the id and both public keys. AES-256-GCM seals the clip, or the file's facts, under that key, with the same info as associated data. A box moved to another id, name or kind doesn't open. Only the recipient can open it, and only the sender could have made it.
- **Files.** Each file gets its own random AES-256 key, which travels only inside the sealed facts. The file is encrypted in 1 MiB chunks with AES-256-GCM. Each chunk's nonce and associated data carry its place and whether it's the last, so chunks can't be reordered, dropped or cut off. The recipient checks the length, the size and the SHA-256 before saving anything.
- **Pinning.** The first time you send to someone, or hear from them, their public key is pinned on your Mac. If the relay later gives a different key, Q Calc refuses to send to them and holds what they sent. Their thread then says "max2's security key changed", with **Trust New Key**. That's expected after they reinstall, or after a name lapses and someone else takes it.
- **Safety code.** Each thread's info (ⓘ) shows a safety code: six groups of five digits from both public keys, the same on both Macs. Compare it with them out loud or in person. If it matches, nobody is in between.

## What the relay keeps, and for how long

| What | Kept | For how long |
| --- | --- | --- |
| A name | A SHA-256 hash of its relay key, when it was last seen, when it was claimed, its public key | Until it's let go, or 30 days unused |
| A file offer | Its id, sender, recipient and encrypted length | Until it's accepted, turned down or taken back, or 7 days |
| A file being sent or waiting | The same, plus where the upload has got to. The encrypted bytes sit in a private R2 bucket. | Deleted once downloaded, taken back or turned down, or 24 hours after upload. A bucket rule removes anything missed within 2 days. |
| Upload counts per address | A SHA-256 tag of the address under a random salt drawn for that hour | The hour. The salt and the tags go when it ends, so a tag can't be traced back to an address. |
| Byte counts per name | Bytes pending per sender and recipient, and per sender per day | Until the bytes are freed, or the day ends |
| A message or clip | Nothing | Passed on, never stored |

The relay logs nothing on these routes, and the Worker's request logs and traces are off (`relay/wrangler.toml`).

## On the Mac

- A received file is decrypted next to Downloads, checked, made non-executable and quarantined (`com.apple.quarantine`). Only then is it moved into Downloads, under a cleaned name that never replaces anything. A file that fails any check is deleted, and nothing partial is left.
- File names are cleaned before sealing and again after opening. Only the last path part is kept. Leading dots, controls, direction marks and invisible characters go. The name is cut to 120 characters, always keeping the extension, so a name can't pass itself off as another type.
- Q Calc never opens, previews or runs a received file. **Show in Finder** shows it only when you press it.
- Size limits are checked on both sides: 1 GB per file and 64 KB per clip.
- Message history (your threads) stays on your Mac, in Q Calc's own storage. It isn't encrypted beyond your disk's own encryption.

## Honest limits

- **Trust on first use.** The first key you see for someone comes from the relay. Whoever runs the relay could hand you a key of their own the first time and read what you send. Only comparing safety codes rules that out.
- **Metadata.** The relay sees who sends to whom, when, and how big each file is. It also sees the address each request comes from. It doesn't keep those addresses, but Cloudflare, which runs it, handles them in passing.
- **Text messages** are readable by the relay as they pass.
- **No forward secrecy for the recipient.** Someone who later gets a recipient's private key from their Mac, and has kept the sealed offers, could open those offers. A stolen sender key doesn't open past shares, since each one also needs an ephemeral key that's thrown away.
- **Your Mac.** Anyone who can use your unlocked Mac can use Q Calc as you, and see what you've saved and your threads.
- **Files are files.** Encryption doesn't make a file safe to open. Quarantine means macOS asks before running anything from one, but think before you open something you weren't expecting.
