# Q Calc relay

A Cloudflare Worker that lets two copies of Q Calc play pong, connect 4, ultimate tic-tac-toe and ember & frost with each other from anywhere, on any network.

Every game has a list of open rooms. A game's lobby watches that list, which shows each room's host by nickname (a random name like "teal otter" that the player can change) and whether the room is locked. A host opens a room either open, which anyone in the list can join with one key, or locked, which needs the 6-digit code the host's lobby shows. A room leaves the list as soon as someone joins it, the host leaves, or 15 minutes pass with nobody joining.

The list holds only a random room id, the nickname and the lock. It never holds an IP address or a code.

## How it fits together

- **Directory**: one Durable Object per game. It keeps that game's open rooms and the lobbies watching them. Watchers are hibernating WebSockets, so an idle lobby costs nothing, and each change pushes the new list to every watcher.
- **Room**: one Durable Object per room. It holds the host's and guest's WebSockets and forwards each side's frames to the other without reading or storing them. The room's id, game, nickname and code ride on the host's socket attachment, which outlasts hibernation. A room adds itself to its game's Directory when it opens and removes itself when it's joined, closed or expired.

| Route (WebSocket) | What it does |
| --- | --- |
| `/rooms/<game>` | Sends the game's open rooms now and again on every change |
| `/host/<game>?name=…&locked=1\|0` | Opens a room. The host hears `hosting {code}`, with `code` empty for an open room |
| `/join/<room>?name=…[&code=…]` | Joins a room from the list |
| `/host`, `/join/<code>` | The code-only rooms of earlier releases, kept so those still pair. They're never listed |
| `/back/<room or code>?token=…` | Takes a seat back after a dropped link (below) |

## Coming back to a game

Mid-game, a player's link can drop: they hid Q Calc, their laptop slept, or the network changed. The room then holds their seat for 10 minutes instead of ending the game. The other player hears `away` and the game waits. When the first player's app reconnects with its seat's token, the other player hears `back`, and the messages sent meanwhile are handed over in order (the newest 256). After 10 minutes the other player hears `closed` (`lost`).

- **Seats.** When two players pair, each `open` carries the room's key and that side's token. The token is 32 random hex characters and only works for its own seat. Seats live in the room's storage, so they outlast hibernation. `relay/src/seats.js` holds the plain rules, tested in `seats.test.js`.
- **Leaving on purpose.** A socket closed with code 1000 still ends the game at once, and the other player hears `bye`. Any other close holds the seat. The app closes with 4000 when it means to come back.
- **Earlier releases.** A page that can take its seat back asks with `resume=1` when it hosts or joins. Seats are kept only when both sides asked, so a game with a 2.0.9 or earlier app ends the moment a link drops, as before.
- **Hiding Q Calc.** The app counts hiding mid-game as stepping away. It closes its link with 4000, the game pauses for both players, and showing Q Calc again takes the seat back. Quitting Q Calc leaves the game outright, since the game itself is gone.

## Limits

Rate limits count per IP address, and an IPv6 address counts by its /64 so one connection can't rotate through addresses. Each can host 10 times a minute, join 20 times a minute and open the list 30 times a minute. A locked room also draws a new code after 5 wrong ones, which the host sees at once, so a code can't be worn down from many addresses. Each game's list keeps at most 500 rooms (the oldest go first) and shows the newest 50.

## Deploy

```bash
cd relay
npx wrangler login
npx wrangler deploy        # prints https://qcalc-relay.<you>.workers.dev
```

For a first deploy, set `RELAY_URL` in `src/lib/peerOnline.ts` to `wss://qcalc-relay.<you>.workers.dev` afterwards and ship a release.

### Deploying seats that outlast a dropped link

This update adds `/back` and the seat rules in `seats.js`. It needs no new bindings, migrations or secrets, because seats use the `Room` class's existing storage. It's backward compatible both ways:

- **Released apps on the new relay** don't ask for seats, so their games behave exactly as before.
- **New apps on the old relay** get no tokens, so a dropped link ends the game as it always has. Once the relay is deployed, seats start working without an app update.

```bash
cd relay
npx wrangler deploy
```

### Deploying the room lists

This update adds the room lists. It's backward compatible both ways, so the relay and the app can update in either order.

- **What changed.** There's a new `Directory` Durable Object class, with one list per game. It comes in as a new `v2` migration, and `v1` is unchanged. There are new `/rooms`, `/host/<game>` and `/join/<room>` routes and a new `LISTS` rate limiter. Rate limits now count an IPv6 address by its /64. A locked room draws a new code after 5 wrong ones.
- **Released apps keep working.** `/host` and `/join/<code>` still serve the old host-shows-code flow, with the same messages as before, and their rooms are never listed.
- **New apps on the old relay.** Until this relay is deployed, the new app's room list gets no answer. Its lobby then says there's no room list yet and falls back to hosting with a code and joining by typing it, through the old routes. Once the relay is updated, the list shows up on its own.

To deploy it:

```bash
cd relay
npx wrangler deploy --dry-run    # optional: checks the config and bundle without uploading
npx wrangler deploy              # applies the v2 migration and the LISTS limiter, then serves both flows
```

Nothing else needs setting up and no data moves. To check it, open two copies of the new app and host a room in one. It should show up in the other's list.

## Cost

Rooms and lists use the WebSocket hibernation API, so a waiting host or an idle lobby costs nothing. The relay answers pings without waking either. A list update is one write to the Directory's storage plus a message to each watcher, and Cloudflare doesn't bill outgoing WebSocket messages. During a game, pong sends about 60–90 frames a second through the room. Cloudflare bills incoming WebSocket messages at 20 per request, which works out to about 4 requests a second per game. The free plan's 100,000 Durable Object requests a day therefore covers about 6 hours of play a day. The $5 Workers Paid plan covers far more.

## Local testing

```bash
npx wrangler dev                                   # ws://localhost:8787
VITE_RELAY_URL=ws://localhost:8787 npm run dev     # from the repo root, in another terminal
```

Without `VITE_RELAY_URL`, `npm run dev` pairs tabs of the same browser instead.
