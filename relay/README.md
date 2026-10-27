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
- **What's kept.** Per name, only the key's hash and a last-seen time, and those only until the name lapses or is let go. Messages pass through and are forgotten. No IP addresses are kept.

| Route | What it does |
| --- | --- |
| `/inbox/<name>` (WebSocket, protocols `qcalc-inbox, k<key>`) | Claims or keeps the name, then sends `inbox {name}` and each `message {from, text}`. A name held by another key hears `closed` (`taken`). |
| `POST /claim {name, key}` | `ok` holds the name for this key; `taken` means another key holds it. |
| `POST /release {name, key}` | Lets the name go if this key holds it, and closes its inbox. |
| `POST /send {from, key, to, text}` | `delivered`, `offline` (no open inbox), `unknown` (nobody holds the name), `not-yours` (the key doesn't hold `from`). A text is up to 500 characters, kept on one line. |
| `POST /lookup {from, key, name}` | `found` or `unknown`, whether anyone holds `name` (what a send already says); `not-yours`, `too-many` (`LOOKUPS`, 60 a minute per address and per sender), `bad`. Read only: it never claims `name` or keeps it from lapsing. The bar checks a name as `message <name>` is typed. |

Each POST takes a JSON body sent as `text/plain`, so a page can send it without a CORS preflight. Each answers `{status}`, and `bad` or `too-many` apply to any of them.

## Files and clips

`paste to max` shares what's on your clipboard with max, and `send file to max` (or dropping a file on the bar) sends a file. Both go only between names, with the same Allow and Block as messages.

- **Clips.** A clip is clipboard text up to 64 KB, sent through `/send` with `kind: 'clip'` and passed on as it is, newlines and all. Like a message, it reaches an open inbox or is turned away, and it's never kept.
- **Offers.** A file is never sent straight away. The sender's app first sends an offer through `/send` with `kind: 'file'` and the file's name, plain size, encrypted length (`bytes`), type, SHA-256 and key. The relay gives it a random 128-bit id, notes it in the ledger and passes it on. An offer holds no room and nothing is stored. One nobody accepts lapses after 7 days.
- **Accepting.** The recipient's app accepts with `POST /file/<id>/accept`. The ledger reserves the file's full length against the caps below and grants the sender one upload of exactly that length, for that sender and that recipient. The sender must be online to hear `accepted`. If they aren't, the reservation is given back and the recipient can accept again later. A grant the sender doesn't start within an hour gives its room back too. The app can accept on its own for accepted names under a size you set (off by default).
- **Uploading.** The sender starts with the grant, which can't be used again, and uploads the file as an R2 multipart upload through the Worker in 50 MiB parts. Every request stays under the Free plan's 100 MB body limit. Every part must be exactly as long as its place in the file, so no byte goes past what was granted, and a part that isn't ends the upload. A part that fails is tried again, up to 3 tries per part in all, and coming back to `/start` says which parts are in. An upload with no part for 2 hours, or unfinished after a day, is aborted. When it's finished the recipient hears `ready`.
- **Downloading.** Only the recipient can download it, with `Range: bytes=N-` to pick up where a cut-short download stopped. It goes as soon as its last byte has been passed on. It's served as `application/octet-stream` with `Content-Disposition: attachment` and `nosniff`. Either side can take it back or turn it down at any point, and the other side hears `cancelled` or `declined`. A file nobody downloads goes 24 hours after it was uploaded.
- **Encryption.** The sender's app encrypts the file with AES-256-GCM under a fresh random key before uploading it. The key travels only inside the offer message. It's never written to R2 or the ledger, and the relay never stores it. The recipient's app checks the SHA-256 after decrypting. This isn't full end-to-end encryption. The relay passes messages on, offers included, so whoever runs the relay could read a key in passing. It does mean what sits in the bucket can't be read on its own.
- **On the Mac.** A received file is saved to Downloads under a new name if one's taken, marked with `com.apple.quarantine`, and shown in Finder. It's never opened or run. Files from someone who hasn't been accepted show in their request but can't be saved until they are.
- **The ledger.** One Durable Object holds every file's state and what it reserves: who sent it to whom, its length and where it's got to, never its name, type or key. A sender's address is kept only as a SHA-256 hash, and only for the hour it's counted in. Its caps are hard, not just rate limits:

| Cap | Limit |
| --- | --- |
| One file | 1 GiB (plus GCM's tags) |
| Pending for one recipient (accepted, uploading or waiting) | 2 GiB |
| Pending from one sender | 2 GiB |
| Uploaded by one sender in a day (counted when accepted) | 5 GiB |
| Pending in the whole bucket | 8 GiB, which keeps it inside R2's 10 GB free tier. When it's full, accepting answers `relay-full` |
| Uploads started from one address in an hour | 20 |
| Offers one sender has out | 50 |
| R2 writes (class A) a day, whole relay | 10,000 |
| R2 reads (class B) a day, whole relay | 50,000 |
| A name's first 10 minutes | It can't offer or upload files, to slow throwaway names |

Room comes back when a file is downloaded, taken back, turned down or expired. A sender's day of uploads isn't given back. A Cron Trigger runs every hour. It lets lapsed offers go, returns unused grants, aborts stale uploads and deletes expired files. Expiry is also checked on every read.

| Route | Who | What it does |
| --- | --- | --- |
| `POST /send {…, kind: 'clip', clip}` | sender | `delivered`, `offline`, `unknown`, as for a message. The inbox hears `message {from, text, kind: 'clip', clip}` |
| `POST /send {…, kind: 'file', file: {name, size, bytes, type, sha256, key}}` | sender | `delivered` with `{id, expires}`, or `offline`, `unknown`, `new-name`, `too-big`, `too-many`. The inbox hears `message {from, text, kind: 'file', file: {id, name, size, bytes, type, sha256, key, expires}}` |
| `POST /file/<id>/accept` | recipient | `ok` (the sender hears `file {event: 'accepted', id, to, grant, bytes, partBytes}`), `sender-offline`, `recipient-full`, `sender-full`, `daily-limit`, `relay-full`, `busy`, `expired`, `gone` |
| `POST /file/<id>/start` (`X-QCalc-Grant`) | sender | `ok {partBytes, parts, done}`. Again without the grant, it says which parts are in. Also `not-accepted`, `too-many`, `new-name`, `expired` |
| `PUT /file/<id>/part/<n>` | sender | One part, exactly `partBytes` long except the last. `ok`, `failed` (try again), `over` (the upload's ended) |
| `POST /file/<id>/finish` | sender | `ok {expires}` (the recipient hears `file {event: 'ready', id, from, expires}`), or `incomplete {missing}` |
| `GET /file/<id>` (`Range: bytes=N-`) | recipient | The encrypted bytes, or `not-ready`, `expired`, `gone` |
| `DELETE /file/<id>` | either | `ok`. The other side hears `file {event: 'cancelled', id, from}` or `file {event: 'declined', id, to}` |

Every `/file` request carries the asker's name and key in `X-QCalc-Name` and `X-QCalc-Key`, never in the URL, and answers JSON `{status}` with a matching HTTP status. An app that only knows text messages shows a clip or an offer as a plain message, "shared their clipboard — update Q Calc to see it" or "sent you a file — update Q Calc to open it". It ignores `file` events.

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

The bucket stays private. Don't give it a public `r2.dev` URL or a custom domain, since the Worker is the only way in. The cron trigger shows under Workers → `qcalc-relay` → Settings → Triggers. Nothing else needs setting up and no data moves. To check it, set different names in two copies of the new app, wait 10 minutes, and send `paste to <other name>`. Then send a file. The other copy shows it, and once it's allowed, Save puts it in Downloads.

## Cost

Rooms and lists use the WebSocket hibernation API, so a waiting host or an idle lobby costs nothing. The relay answers pings without waking either. A locked room adds one request to claim its code and one to let it go, and joining by a code alone adds one hop on the way in. A list update is one write to the Directory's storage plus a message to each watcher, and Cloudflare doesn't bill outgoing WebSocket messages. During a game, pong sends about 60–90 frames a second through the room. Cloudflare bills incoming WebSocket messages at 20 per request, which works out to about 4 requests a second per game. The free plan's 100,000 Durable Object requests a day therefore covers about 6 hours of play a day. The $5 Workers Paid plan covers far more.

A waiting inbox hibernates too, and its 30-second pings are answered without waking it. A message costs two Durable Object requests: one to check the sender's name and one to hand it over. A name's last-seen time is written at most once a day.

Files cost R2 storage and operations, plus Worker and Durable Object requests. R2's free tier is 10 GB-month of storage, 1 million class A operations (writes) and 10 million class B operations (reads) a month, with free egress. Deleting objects and aborting uploads cost nothing.

- **One file.** A file up to 50 MiB takes 3 class A operations (start, one part, finish) and 1 class B (the download). A 1 GiB file takes 23 class A, for its 21 parts, and 1 class B. It also takes about 3 Durable Object requests per part, for the sender's name check, the ledger before and the ledger after, plus a few for the offer, accept, start, finish and download.
- **Storage.** The bucket's pending bytes are capped at 8 GiB, reserved from the moment a file is accepted, unfinished parts included. Each file lives at most 24 hours after it's uploaded. Even with the bucket full all month, storage stays at 8 GB-month, under the free 10.
- **Operations.** The ledger allows 10,000 class A operations a day across the whole relay, about 300,000 a month, and 50,000 class B a day, about 1.5 million a month. Both are well under the free tier however many names or addresses send. When a day's budget is spent, accepting, uploading or downloading answers `relay-full` until the next UTC day.
- **The worst case.** Someone set on running it up would need two names they control, each 10 minutes old. Their address can start 20 uploads an hour, and each name can move 5 GiB a day. However many names and addresses they spread across, the whole relay stops at 8 GiB stored and 10,000 writes and 50,000 reads a day, so the R2 bill stays at $0. What they can use up is other people's room for files for the rest of that day. The 10,000 writes a day also hold Durable Object requests from files to roughly 30,000 a day, which leaves most of the free plan's 100,000 a day for messages and games. Offers and accepts that never become uploads cost one Durable Object request each and no R2 operations, and they're held by `UPLOADS` (10 a minute per address and per name), `FILE_GETS` (30 a minute per address) and the cap of 50 offers out per sender.

## Local testing

```bash
npx wrangler dev                                   # ws://localhost:8787, with a local bucket and ledger
VITE_RELAY_URL=ws://localhost:8787 npm run dev     # from the repo root, in another terminal
```

Without `VITE_RELAY_URL`, `npm run dev` pairs tabs of the same browser instead.
