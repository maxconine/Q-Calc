# Q Calc relay

A Cloudflare Worker that lets two copies of Q Calc play pong, connect 4, ultimate tic-tac-toe and ember & frost with each other from anywhere, on any network. It also passes short messages, clipboard text and files between Q Calc users ([below](#messages)).

Every game has a list of open rooms. A game's lobby watches that list, which shows each room's host by nickname (a random name like "teal otter" that the player can change) and whether the room is locked. A host opens a room either open, which anyone in the list can join with one key, or locked, which needs the 6-digit code the host's lobby shows. A friend can also join a locked room by typing its code alone, without picking it from the list. A room leaves the list as soon as someone joins it, the host leaves, or 15 minutes pass with nobody joining.

The list holds only a random room id, the nickname and the lock. It never holds an IP address or a code. A locked room's code is held apart from the list, in that code's own Room object, only until the room is joined, closed or expired.

## How it fits together

- **Directory**: one Durable Object per game. It keeps that game's open rooms and the lobbies watching them. Watchers are hibernating WebSockets, so an idle lobby costs nothing, and each change pushes the new list to every watcher.
- **Room**: one Durable Object per room. It holds the host's and guest's WebSockets and forwards each side's frames to the other without reading or storing them. The room's id, game, nickname and code ride on the host's socket attachment, which outlasts hibernation. A room adds itself to its game's Directory when it opens and removes itself when it's joined, closed or expired.
- **Code index**: a locked room claims its code in the Room object named by that code, the same one an earlier release's code-only room with that code would use. That Room keeps a small claim (the room id, the code and when it lapses) in storage and forwards `/join/<code>` to the listed room, which checks the code as it does any other. A code is claimed only when its Room has no code-only host, no code-only game whose player is away or that ended in the last 10 minutes, and no other room's claim, and a code-only host isn't let in while a claim stands, so no two rooms of either kind share a code. The claim goes when the room is joined, closed or expired, or when it draws a new code after 5 wrong ones. If letting it go fails, the claim lapses when the room would have expired anyway.
- **Ledger**: one Durable Object for every file in flight and the bytes they hold against the caps ([below](#files-and-clips)). `relay/src/files.js` holds the plain rules and `files.test.js` tests both.
- **Inbox**: one Durable Object per messaging name. It holds who has the name (a hash of their key and when they were last seen) and their app's hibernating sockets, and it hands each message to those sockets without keeping it. `relay/src/names.js` holds the plain rules, which `names.test.js` tests, and `inbox.test.js` tests the routes.

| Route (WebSocket) | What it does |
| --- | --- |
| `/rooms/<game>` | Sends the game's open rooms now and again on every change |
| `/host/<game>?name=…&locked=1\|0` | Opens a room. The host hears `hosting {code}`, with `code` empty for an open room |
| `/join/<room>?name=…[&code=…]` | Joins a room from the list |
| `/join/<code>[?name=…]` | Joins by the code alone: a locked listed room holding that code, or else a code-only room of an earlier release |
| `/host` | The code-only rooms of earlier releases, kept so those still pair. They're never listed |
| `/back/<room or code>?token=…` | Takes a seat back after a dropped link (below) |

## Messages

Typing `message max hello` in the bar sends "hello" to whoever holds the name max. Their Mac shows a notification, "ana: hello". On Windows and in the browser it shows under the bar.

- **Names.** Messaging is off until you set a name in settings. A name is 2–20 lowercase letters, digits, `-` or `_`. The app makes a random 32-byte key the first time and keeps it. Claiming a name ties it to that key. The relay keeps only a SHA-256 hash of the key and when the name was last seen, so two people can't hold one name at once. A name nobody has used for 30 days lapses, and anyone can take it then. Changing your name lets the old one go straight away, and so does turning messages off.
- **Receiving.** With a name set, the app keeps one WebSocket open to that name's inbox. The Mac app holds it natively whether or not the bar is up. Windows and the browser hold it while the bar's page runs. The key is offered as a WebSocket protocol, never in the URL. The socket hibernates on the relay, so a waiting inbox costs nothing.
- **Who gets through.** The lists live only in the app. A message from a name you've accepted shows as it is. Someone new's first message shows only as a request, "ana wants to message you", with Allow and Block. Further messages from them don't ask again. Allowing them shows the last thing they said. A blocked name's messages are dropped without being shown. Accepted and blocked names can be edited in settings.
- **Sending.** The sender proves their own name with their key, so nobody can write as someone else. Nothing is queued. A message reaches the name's open inbox or is turned away, and the sender's bar says which: sent, not online, or nobody's called that. The sender is never told whether they're accepted or blocked.
- **What's kept.** Per name, only the key's hash, a last-seen time and, from the native app, its public key, and those only until the name lapses or is let go. Messages pass through and are forgotten. No IP addresses are kept.
- **Not end-to-end.** Text messages are passed on as they are, so every copy of Q Calc, old or new, can read them. Clips and files are end-to-end encrypted ([below](#files-and-clips), and [native/PRIVACY.md](../native/PRIVACY.md)).

| Route | What it does |
| --- | --- |
| `/inbox/<name>` (WebSocket, protocols `qcalc-inbox, k<key>`) | Claims or keeps the name, then sends `inbox {name}` and each `message {from, text}`. A name held by another key hears `closed` (`taken`). |
| `POST /claim {name, key}` | `ok` holds the name for this key; `taken` means another key holds it. |
| `POST /release {name, key}` | Lets the name go if this key holds it, and closes its inbox. |
| `POST /send {from, key, to, text}` | `delivered`, `offline` (no open inbox), `unknown` (nobody holds the name), `not-yours` (the key doesn't hold `from`). A text is up to 500 characters, kept on one line. |
| `POST /lookup {from, key, name}` | `found` with `pub` (the name's public key, `''` when it has none) or `unknown`, whether anyone holds `name` (what a send already says); `not-yours`, `too-many` (`LOOKUPS`, 60 a minute per address and per sender), `bad`. Read only: it never claims `name` or keeps it from lapsing. The bar checks a name as `message <name>` is typed. |
| `POST /pubkey {name, key, pub}` | `ok` publishes the name's X25519 public key (32 bytes, base64url) for end-to-end clips and files; `not-yours` when the key doesn't hold the name, `bad` when `pub` isn't a key. A new holder of a lapsed name starts without one. |

Each POST takes a JSON body sent as `text/plain`, so a page can send it without a CORS preflight. Each answers `{status}`, and `bad` or `too-many` apply to any of them.

## Files and clips

`paste to max` shares what's on your clipboard with max, and `send file to max` (or dropping files on the bar while it says `message max…` or `send file to max`) sends a file. Both go only between names, with the same Allow and Block as messages.

Both are end-to-end encrypted by the native app: the relay can't read a clip or a file, or a file's name, type, size, hash or key. It sees who sent to whom, when, and a file's encrypted length. [native/PRIVACY.md](../native/PRIVACY.md) has the details, what's kept and for how long, and the limits.

- **Clips.** A clip is clipboard text up to 64 KB, sealed for its recipient and sent through `/send` with `kind: 'clip'` and `clip: {id, epk, sealed}`: an id the sender drew, its ephemeral public key and the sealed box. The relay checks only their shape and size. Like a message, it reaches an open inbox or is turned away, and it's never kept. A plain clip is turned away.
- **Offers.** A file is never sent straight away. The sender's app first sends an offer through `/send` with `kind: 'file'` and `file: {id, epk, sealed, bytes}`: a random 128-bit id it drew, its ephemeral public key, the file's facts sealed for the recipient (name, type, size, SHA-256 and the file's own key), and the encrypted length the relay has to hold. An id that's already out is turned away. The relay notes the offer in the ledger and passes it on. An offer holds no room and nothing is stored. One nobody accepts lapses after 7 days.
- **Accepting.** The recipient's app accepts with `POST /file/<id>/accept`. The ledger reserves the file's full length against the caps below and grants the sender one upload of exactly that length, for that sender and that recipient. The sender must be online to hear `accepted`. If they aren't, the reservation is given back and the recipient can accept again later. A grant the sender doesn't start within an hour gives its room back too. The app can accept on its own for accepted names under a size you set (off by default on the Mac; up to 100 MB by default on Windows). The answer, and the sender's `accepted`, carry `stream`: the part length a streamed upload uses.
- **Uploading.** The sender starts with the grant, which can't be used again, and uploads the file as an R2 multipart upload through the Worker in 50 MiB parts. Every request stays under the Free plan's 100 MB body limit. Every part must be exactly as long as its place in the file, so no byte goes past what was granted, and a part that isn't ends the upload. A part that fails is tried again, up to 3 tries per part in all, and coming back to `/start` says which parts are in. An upload with no part for 2 hours, or unfinished after a day, is aborted. When it's finished the recipient hears `ready`.
- **Streaming.** A native app from 2026-10 on streams instead: its first part carries the grant (`X-QCalc-Grant` with `X-QCalc-Stream: 1`) and starts the upload with no `/start` of its own, and each part is an R2 object of its own (`f/<id>/<n>`), not part of a multipart upload. The recipient hears `file {event: 'part', id, n}` as each lands and reads it at once with `GET /file/<id>/part/<n>`, so the download runs alongside the upload instead of after it. A part is a whole number of the app's sealed chunks (1 MiB and its 16-byte tag): 4 of them, or more for a big file so no file takes more than 64 parts. The recipient's app writes them in order into one hidden file beside Downloads, opening each chunk as it comes, and moves the finished, checked file in at once; nothing partial is left there. The file goes once every part has been read and the upload's finished. An app that doesn't stream still works both ways: an older sender's upload is a multipart upload as before (a streaming recipient asking for a part hears `whole` and waits for `ready`), and an older recipient reads a streamed file whole with `GET /file/<id>`, its parts one after another.
- **Downloading.** Only the recipient can download it, with `Range: bytes=N-` to pick up where a cut-short download stopped. It goes as soon as its last byte has been passed on. It's served as `application/octet-stream` with `Content-Disposition: attachment` and `nosniff`. Either side can take it back or turn it down at any point, and the other side hears `cancelled` or `declined`. A file nobody downloads goes 24 hours after it was uploaded.
- **Encryption.** The sender's app encrypts the file with AES-256-GCM under a fresh random key before uploading it. That key travels only inside the sealed facts, which only the recipient's app can open: each name's app has an X25519 key pair, publishes the public half with `/pubkey`, and seals with HKDF-SHA256 over two agreements bound to both names, the id and both keys. Each app pins a contact's key the first time and refuses to send if it changes; each thread shows a safety code to compare. The recipient's app checks the length, size and SHA-256 after decrypting.
- **On the Mac.** A received file is decrypted next to Downloads, checked, made non-executable and quarantined (`com.apple.quarantine`), then moved into Downloads under a new name if one's taken. It's never opened, previewed or run, and it's shown in Finder only when you ask. Files from someone who hasn't been accepted show in their request but can't be saved until they are.
- **The ledger.** One Durable Object holds every file's state and what it reserves: who sent it to whom, its length and where it's got to. It never sees a name, type or key. An address is never kept: an upload's start counts against a SHA-256 tag of the address under a random salt drawn for that hour, and the salt and the tags go when the hour ends. Each ledger call runs without another in between, and the steps around it are checked for races: a take-back during an accept, a part or a finish leaves nothing behind. Its caps are hard, not just rate limits:

| Cap | Limit |
| --- | --- |
| One file | 1 GiB (plus GCM's 16-byte tag per 1 MiB chunk) |
| Pending for one recipient (accepted, uploading or waiting) | 2 GiB |
| Pending from one sender | 2 GiB |
| Uploaded by one sender in a day (counted when accepted) | 5 GiB |
| Pending in the whole bucket | 8 GiB, which keeps it inside R2's 10 GB free tier. When it's full, accepting answers `relay-full` |
| Uploads started from one address in an hour | 20 |
| Offers one sender has out | 50 |
| R2 writes (class A) a day, whole relay | 10,000 |
| R2 reads (class B) a day, whole relay | 50,000 |

Room comes back when a file is downloaded, taken back, turned down or expired. A sender's day of uploads isn't given back. A Cron Trigger runs every hour. It lets lapsed offers go, returns unused grants, aborts stale uploads and deletes expired files. Expiry is also checked on every read.

| Route | Who | What it does |
| --- | --- | --- |
| `POST /send {…, kind: 'clip', clip: {id, epk, sealed}}` | sender | `delivered`, `offline`, `unknown`, as for a message. The inbox hears `message {from, text, kind: 'clip', clip}` |
| `POST /send {…, kind: 'file', file: {id, epk, sealed, bytes}}` | sender | `delivered` with `{id, expires}`, or `offline`, `unknown`, `new-name`, `too-big`, `too-many`, `bad` (also for an id that's out already). The inbox hears `message {from, text, kind: 'file', file: {id, epk, sealed, bytes, expires}}` |
| `POST /file/<id>/accept` | recipient | `ok {stream}` (the sender hears `file {event: 'accepted', id, to, grant, bytes, partBytes, stream}`), `sender-offline`, `recipient-full`, `sender-full`, `daily-limit`, `relay-full`, `busy`, `expired`, `gone` |
| `POST /file/<id>/start` (`X-QCalc-Grant`) | sender | `ok {partBytes, parts, done}`. Again without the grant, it says which parts are in (and `stream` for a streamed upload, whose `partBytes` is its own). Also `not-accepted`, `too-many`, `new-name`, `expired` |
| `PUT /file/<id>/part/<n>` | sender | One part, exactly `partBytes` long except the last. `ok`, `failed` (try again), `over` (the upload's ended). Streamed: exactly `stream` long except the last; the first with `X-QCalc-Grant` and `X-QCalc-Stream: 1` starts it (as `/start` would, counted the same); the recipient hears `file {event: 'part', id, n}` |
| `GET /file/<id>/part/<n>` | recipient | A streamed part, as soon as it's in: the encrypted bytes, or `not-ready` (not in yet), `whole` (not streamed: wait for `ready`), `expired`, `gone` |
| `POST /file/<id>/finish` | sender | `ok {expires}` (the recipient hears `file {event: 'ready', id, from, expires}`, unless they've streamed every part already, when it goes at once), or `incomplete {missing}` |
| `GET /file/<id>` (`Range: bytes=N-`) | recipient | The encrypted bytes (a streamed file's parts one after another), or `not-ready`, `expired`, `gone` |
| `DELETE /file/<id>` | either | `ok`. The other side hears `file {event: 'cancelled', id, from}` or `file {event: 'declined', id, to}` |

Every `/file` request carries the asker's name and key in `X-QCalc-Name` and `X-QCalc-Key`, never in the URL, and answers JSON `{status}` with a matching HTTP status. Only the sender can start, upload, finish or take back a file, and only the recipient can accept, download or turn it down. Anyone else, and an id that doesn't exist, hears the same `gone`, so ids can't be probed. A grant works once. An R2 key is only ever `f/` and a 32-hex id, and for a streamed part `/` and its number (1 to 64). A part must declare its length, which must be exactly its place's; a body that's shorter or longer is cut off by `FixedLengthStream` and tried again. An app that only knows text messages shows a clip or an offer as a plain message, "shared their clipboard — update Q Calc to see it" or "sent you a file — update Q Calc to open it". It ignores `file` events.

## Coming back to a game

Mid-game, a player's link can drop: they hid Q Calc, their laptop slept, or the network changed. The room then holds their seat for 10 minutes instead of ending the game. The other player hears `away` and the game waits. When the first player's app reconnects with its seat's token, the other player hears `back`, and the messages sent meanwhile are handed over in order (the newest 256). After 10 minutes the other player hears `closed` (`lost`).

- **Seats.** When two players pair, each `open` carries the room's key and that side's token. The token is 32 random hex characters and only works for its own seat. Seats live in the room's storage, so they outlast hibernation. `relay/src/seats.js` holds the plain rules, tested in `seats.test.js`.
- **Leaving on purpose.** A socket closed with code 1000 still ends the game at once, and the other player hears `bye`. Any other close holds the seat. The app closes with 4000 when it means to come back.
- **Earlier releases.** A page that can take its seat back asks with `resume=1` when it hosts or joins. Seats are kept only when both sides asked, so a game with a 2.0.9 or earlier app ends the moment a link drops, as before.
- **Hiding Q Calc.** The app counts hiding mid-game as stepping away. It closes its link with 4000, the game pauses for both players, and showing Q Calc again takes the seat back. Quitting Q Calc leaves the game outright, since the game itself is gone.
- **Seats and codes.** A seat comes back only through `/back` with its token, never through a code, so a player coming back is never taken for a new guest and never counts as a wrong code. A guest who joined by typing a locked room's code gets a seat in that listed room, keyed by its room id. While a seat is held, nobody else can take it: the room answers `busy`. A held seat comes only after pairing, and pairing lets a locked room's code go, so a code is never held for a game in progress. A host whose link drops before anyone joins closes the room as before, which lets its code go too.

## Limits

Rate limits count per IP address, and an IPv6 address counts by its /64 so one connection can't rotate through addresses. Each can host 10 times a minute, join 20 times a minute and open the list 30 times a minute. A locked room also draws a new code after 5 wrong ones, which the host sees at once, so a code can't be worn down from many addresses. Typing a code alone counts against the join limit, the same as it always has for code-only rooms. Each game's list keeps at most 500 rooms (the oldest go first) and shows the newest 50.

For messages, each address can claim names and open inboxes 20 times a minute and send 30 messages a minute. Each name can send 10 a minute, whatever address it sends from. A name keeps at most 3 inboxes open, and a 4th closes the oldest.

## Deploy

```bash
cd relay
npx wrangler login
npx wrangler deploy        # prints https://qcalc-relay.<you>.workers.dev
```

For a first deploy, set `RELAY_URL` in `src/lib/peerOnline.ts` to `wss://qcalc-relay.<you>.workers.dev` afterwards and ship a release.

### Deploying the room lists, joining by code and seats

The live relay still predates the room lists: it answers 404 on `/rooms`. This update brings it up to date in one deploy, adding the room lists, joining a locked room by its code alone, and seats that outlast a dropped link. It's backward compatible both ways, so the relay and the app can update in either order.

- **What changed.** There's a new `Directory` Durable Object class, with one list per game. It comes in as a new `v2` migration, and `v1` is unchanged. There are new `/rooms`, `/host/<game>`, `/join/<room>` and `/back` routes and a new `LISTS` rate limiter. Rate limits now count an IPv6 address by its /64. A locked room draws a new code after 5 wrong ones. `/join/<code>` now also reaches a locked listed room, through the code index kept in the existing `Room` class. Seats and the code index both live in the `Room` class's existing storage, so neither needs a class, binding, migration or secret of its own.
- **Released apps keep working.** `/host` and `/join/<code>` still serve the old host-shows-code flow, with the same messages as before, and their rooms are never listed. A released app that types a new app's locked-room code joins that room too. Released apps don't ask for seats, so their games end when a link drops, as before.
- **New apps on the old relay.** Until this relay is deployed, the new app's room list gets no answer. Its lobby then says there's no room list yet and falls back to hosting with a code and joining by typing it, through the old routes, which ignore the `name` and `resume` the new app adds. The old relay gives no tokens, so a dropped link ends the game as it always has. Once the relay is updated, the list and seats start working on their own.

To deploy it:

```bash
cd relay
npx wrangler deploy --dry-run    # optional: checks the config and bundle without uploading
npx wrangler deploy              # applies the v2 migration and the LISTS limiter, then serves both flows
```

Nothing else needs setting up and no data moves. To check it, open two copies of the new app and host a room in one. It should show up in the other's list. Then host a locked room and, in the other copy, press `j` and type its code. It should join without picking the room from the list. Mid-game, hide one copy for a moment: the other should pause, and showing it again should carry on.

### Deploying messages

This adds the `Inbox` Durable Object class as a new `v3` migration, and `v1` and `v2` are unchanged. It also adds the `NAMES`, `MESSAGES` and `SENDERS` rate limiters and the `/inbox`, `/claim`, `/release` and `/send` routes. Nothing that games use changes. The later `/lookup` route needs the `LOOKUPS` rate limiter too; until that's deployed the bar's name check says nothing (no green, no "not found"). Until it's deployed, a new app's name can't be saved (settings says the relay can't be reached), and `message` says it couldn't reach the relay. Nothing else breaks.

```bash
cd relay
npx wrangler deploy --dry-run    # optional
npx wrangler deploy              # applies the v3 migration and the three limiters
```

To check it, set a different name in each of two copies of the new app. Send `message <other name> hi` from one. The other should get a request. Allow it, and the message shows.

### Deploying files and clips

This adds the `Ledger` Durable Object class as a new `v4` migration, and `v1` to `v3` are unchanged. It also adds the `FILES` R2 bucket binding, an hourly Cron Trigger, the `UPLOADS` and `FILE_GETS` rate limiters and the `/file/…` routes. Clips need none of it beyond the new Worker code. Until it's deployed, the old relay answers a clip or an offer with `bad`, since it has no text, and a new app's `paste to` and `send file to` say the relay needs updating. Nothing else breaks.

1. Create the bucket. Its name must match `bucket_name` in `wrangler.toml`.

   ```bash
   cd relay
   npx wrangler r2 bucket create qcalc-files
   ```

   R2 has to be turned on for the account once (dashboard → R2 → Purchase R2 / Enable). That asks for a payment method even on the free tier, but nothing's billed inside it.

2. Add the lifecycle rules as a backstop to the hourly sweep. Expire objects under `f/` after 2 days. A file's life is 24 hours after upload, and a rule only counts whole days, so 2 days catches anything the sweep missed. Abort incomplete multipart uploads after 1 day.

   ```bash
   npx wrangler r2 bucket lifecycle add qcalc-files expire-files f/ --expire-days 2
   npx wrangler r2 bucket lifecycle add qcalc-files abort-uploads f/ --abort-multipart-days 1
   npx wrangler r2 bucket lifecycle list qcalc-files
   ```

   Or in the dashboard: R2 → `qcalc-files` → Settings → Object lifecycle rules → Add rule. Prefix `f/`, "Delete uploaded objects after 2 days", and a second rule with "Abort incomplete multipart uploads after 1 day".

3. Deploy:

   ```bash
   npx wrangler deploy --dry-run    # optional
   npx wrangler deploy              # applies v4, binds the bucket, the limiters and the hourly cron
   ```

The bucket stays private. Don't give it a public `r2.dev` URL or a custom domain, since the Worker is the only way in. The cron trigger shows under Workers → `qcalc-relay` → Settings → Triggers. Nothing else needs setting up and no data moves. To check it, set different names in two copies of the new app and send `paste to <other name>`. Then send a file. The other copy shows it, and once it's allowed, Save puts it in Downloads.

### Deploying end-to-end clips and files

This adds `/pubkey`, a public key on each name's existing claim, sealed clips and offers on `/send`, salted address tags in the ledger and `[observability] enabled = false`. There's no new class, binding, migration, limiter or secret. `npx wrangler deploy` is all it takes.

Once it's deployed, a native build from before end-to-end encryption gets `bad` for a clip or a file, and says it couldn't be sent. Text messages, games and the web app don't change. A new native build on the old relay can't publish its key (`/pubkey` is a 404), so it says the other person "needs a newer q calc". Deploy the relay first, then ship the app.

### Deploying streamed files

This adds `stream` to the accept's answer and the sender's `accepted`, the streamed first part (`X-QCalc-Stream`, which the file routes' CORS now allow), `GET /file/<id>/part/<n>`, the `part` event, and streamed files kept as `f/<id>/<n>` objects. There's no new class, binding, migration, limiter or secret, and the ledger's records gain fields without a migration. `npx wrangler deploy` is all it takes; the existing `f/` lifecycle rules cover the part objects too.

Nothing shipped breaks. The Mac and Windows apps out now never send `X-QCalc-Stream`, so their uploads stay multipart uploads, and they ignore `stream` and the `part` event; a file streamed to one of them is read whole as before. A Windows build that streams falls back by itself on the old relay (no `stream` in `accepted`), so the order of deploying and shipping doesn't matter. A file being uploaded when the relay is deployed finishes the way it started.

For the Mac to stream too, MessageFiles.swift would: upload with `stream` from `accepted` when it's there (the first part with the grant and `X-QCalc-Stream: 1`, no `/start`, then the rest, several at once); and, when the accept's answer has `stream`, read `GET /file/<id>/part/<n>` in order as `part` events come (or every second and a half), opening each sealed chunk as it lands into one hidden file beside Downloads, falling back to `ready` and the whole download on `whole`. FileCrypt.swift would need a chunk-at-a-time opener like share_crypt.rs's `StreamOpen`.

## Cost

Rooms and lists use the WebSocket hibernation API, so a waiting host or an idle lobby costs nothing. The relay answers pings without waking either. A locked room adds one request to claim its code and one to let it go, and joining by a code alone adds one hop on the way in. A list update is one write to the Directory's storage plus a message to each watcher, and Cloudflare doesn't bill outgoing WebSocket messages. During a game, pong sends about 60–90 frames a second through the room. Cloudflare bills incoming WebSocket messages at 20 per request, which works out to about 4 requests a second per game. The free plan's 100,000 Durable Object requests a day therefore covers about 6 hours of play a day. The $5 Workers Paid plan covers far more.

A waiting inbox hibernates too, and its 30-second pings are answered without waking it. A message costs two Durable Object requests: one to check the sender's name and one to hand it over. A name's last-seen time is written at most once a day.

Files cost R2 storage and operations, plus Worker and Durable Object requests. R2's free tier is 10 GB-month of storage, 1 million class A operations (writes) and 10 million class B operations (reads) a month, with free egress. Deleting objects and aborting uploads cost nothing.

- **One file.** A file up to 50 MiB takes 3 class A operations (start, one part, finish) and 1 class B (the download). A 1 GiB file takes 23 class A, for its 21 parts, and 1 class B. Streamed, a file takes 1 class A and 1 class B per part and none to start or finish: 1 each for a file up to 4 MiB, 5 each for 20 MB, 48 each for 200 MB, and 64 each for 1 GiB. Read whole by an app that doesn't stream, it's 1 class B per part. It also takes about 3 Durable Object requests per part, for the sender's name check, the ledger before and the ledger after, plus a few for the offer, accept, start, finish and download.
- **Storage.** The bucket's pending bytes are capped at 8 GiB, reserved from the moment a file is accepted, unfinished parts included. Each file lives at most 24 hours after it's uploaded. Even with the bucket full all month, storage stays at 8 GB-month, under the free 10.
- **Operations.** The ledger allows 10,000 class A operations a day across the whole relay, about 300,000 a month, and 50,000 class B a day, about 1.5 million a month. Both are well under the free tier however many names or addresses send. When a day's budget is spent, accepting, uploading or downloading answers `relay-full` until the next UTC day.
- **The worst case.** Someone set on running it up would need two names they control (a name can send files as soon as it's claimed; `NEW_NAME_MS` in `files.js` brings back a wait). Their address can start 20 uploads an hour, and each name can move 5 GiB a day. However many names and addresses they spread across, the whole relay stops at 8 GiB stored and 10,000 writes and 50,000 reads a day, so the R2 bill stays at $0. What they can use up is other people's room for files for the rest of that day. The 10,000 writes a day also hold Durable Object requests from files to roughly 30,000 a day, which leaves most of the free plan's 100,000 a day for messages and games. Offers and accepts that never become uploads cost one Durable Object request each and no R2 operations, and they're held by `UPLOADS` (10 a minute per address and per name), `FILE_GETS` (30 a minute per address) and the cap of 50 offers out per sender.

## Local testing

```bash
npx wrangler dev                                   # ws://localhost:8787, with a local bucket and ledger
VITE_RELAY_URL=ws://localhost:8787 npm run dev     # from the repo root, in another terminal
```

Without `VITE_RELAY_URL`, `npm run dev` pairs tabs of the same browser instead.
