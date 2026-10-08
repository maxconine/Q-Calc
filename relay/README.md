# Q Calc relay

A Cloudflare Worker that lets two copies of Q Calc play pong (and later games) with each other from anywhere, on any network. One player hosts and gets a 6-digit code, and the other types that code to join.

Each code gets its own Durable Object room. The room holds the two WebSockets and forwards each side's frames to the other without reading or storing them. A room nobody joins closes after 15 minutes. Each IP address can host 10 times a minute and try 20 codes a minute.

## Deploy

```bash
cd relay
npx wrangler login
npx wrangler deploy        # prints https://qcalc-relay.<you>.workers.dev
```

Then set `RELAY_URL` in `src/lib/peerOnline.ts` to `wss://qcalc-relay.<you>.workers.dev` and ship a release.

## Cost

Rooms use the WebSocket hibernation API, so a waiting host costs nothing. During a game, pong sends about 60–90 frames a second through the room. Cloudflare bills incoming WebSocket messages at 20 per request, which works out to about 4 requests a second per game. The free plan's 100,000 Durable Object requests a day therefore covers about 6 hours of play a day. The $5 Workers Paid plan covers far more.

## Local testing

```bash
npx wrangler dev                                   # ws://localhost:8787
VITE_RELAY_URL=ws://localhost:8787 npm run dev     # from the repo root, in another terminal
```

Without `VITE_RELAY_URL`, `npm run dev` pairs tabs of the same browser instead.
