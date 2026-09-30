# Q Calc analytics

A Cloudflare Worker with a D1 database that collects anonymous usage counts from the Mac and Windows apps, takes a daily snapshot of GitHub release download counts, and serves a password-protected dashboard. It fits in Cloudflare's free tier.

## What the apps send

The apps send at most one small POST every 6 hours, and only when there is something new to send:

```json
{ "id": "<random 32 hex>", "platform": "mac", "version": "2.0.4", "os": "macOS 15.5",
  "days": { "2026-09-29": { "open": 12, "calc.units": 4, "copy.answer": 3 } } }
```

- The `id` is random and made on the device. It isn't tied to the machine or the person, and turning sharing off deletes it.
- Only event names from `USAGE_EVENTS` in `src/lib/analytics.ts` are sent. Expressions, answers and history never leave the device.
- To opt out: Settings → **Share anonymous usage**.

## Set up (once)

```bash
cd analytics
cp wrangler.example.toml wrangler.toml          # wrangler.toml is gitignored, so the database id stays local
npx wrangler login
npx wrangler d1 create qcalc-analytics          # paste the database_id into wrangler.toml
npx wrangler d1 execute qcalc-analytics --remote --file schema.sql
npx wrangler secret put DASHBOARD_PASSWORD
npx wrangler deploy                              # prints https://qcalc-analytics.<you>.workers.dev
```

Then set `ANALYTICS_URL` in `src/lib/analytics.ts` to `https://qcalc-analytics.<you>.workers.dev/e` and ship a release. While it's empty, the apps record nothing and send nothing.

Optional: `npx wrangler secret put GITHUB_TOKEN` (a read-only token) avoids GitHub's anonymous API rate limit.

## Dashboard

Open the worker URL and sign in with any username and your `DASHBOARD_PASSWORD`. The dashboard shows:

- daily active installs
- downloads per day (Mac and Windows)
- calculations per day
- new installs
- feature usage
- versions in use

It covers 7, 30 or 90 days, or a year.

- Downloads come from GitHub's running totals, snapshotted daily at 06:17 UTC. The first snapshot is the baseline, so the per-day chart starts filling in the day after deploy. Mac `.zip` counts include Sparkle auto-updates. Use **snapshot now** to take one immediately.
- Apps date their counts by their own local day and report every few hours, so today's numbers fill in late.

## Local testing

```bash
echo DASHBOARD_PASSWORD=test > .dev.vars
npx wrangler d1 execute qcalc-analytics --local --file schema.sql
npx wrangler dev --test-scheduled               # curl localhost:8787/__scheduled runs the snapshot
```
