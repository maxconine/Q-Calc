#!/usr/bin/env python3
"""Tables from the bench results: `summarize.py engine <dir>` (engine-*.json), `summarize.py app <dir>` (app-*.json).
Repeated runs are shown as median [min–max]; keystroke latencies pool every run."""
import json, os, statistics, sys


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))] if xs else float("nan")


def spread(xs, fmt="{:.0f}"):
    xs = [x for x in xs if x is not None]
    if not xs:
        return "n/a"
    if len(xs) == 1:
        return fmt.format(xs[0])
    return f"{fmt.format(statistics.median(xs))} [{fmt.format(min(xs))}–{fmt.format(max(xs))}]"


def dist(xs):
    return f"{pct(xs, 50):.2f} / {pct(xs, 90):.2f} / {pct(xs, 99):.2f} / {max(xs):.1f}"


def engine(d):
    runs = {}
    for name in ("rust", "jsc", "node"):
        path = f"{d}/engine-{name}.json"
        if os.path.exists(path):
            runs[name] = json.load(open(path))
    names = list(runs)
    print("| | " + " | ".join(names) + " |")
    print("|---|" + "---|" * len(names))
    print("| first call (`1+1`, lazy tables), ms | " + " | ".join(f"{runs[n]['firstCall']:.1f}" for n in names) + " |")
    print("| keystroke ms: median / p90 / p99 / max | " + " | ".join(dist(runs[n]["keys"]) for n in names) + " |")
    print("| all keystrokes, first time each string is seen, ms | " + " | ".join(f"{runs[n].get('firstPass', 0):.0f}" for n in names) + " |")
    print("| all keystrokes once, warm, ms | " + " | ".join(f"{runs[n]['keyTotal']:.0f}" for n in names) + " |")
    for i, h in enumerate(runs[names[0]]["heavy"]):
        cells = []
        for n in names:
            x = runs[n]["heavy"][i]
            cells.append(f"{x['cold']:.1f} / {statistics.median(x['warm']):.2f}")
        print(f"| {h['name']}, cold / warm median ms | " + " | ".join(cells) + " |")
    print()
    for i, h in enumerate(runs[names[0]]["heavy"]):
        print(f"- {h['name']}: " + "; ".join(f"{n} `{runs[n]['heavy'][i].get('answer', '?')}`" for n in names))


def app(d):
    runs = {k: json.load(open(f"{d}/app-{k}.json")) for k in ("html", "native") if os.path.exists(f"{d}/app-{k}.json")}
    names = list(runs)
    rows = []

    def row(label, f):
        rows.append(f"| {label} | " + " | ".join(f(runs[n]) for n in names) + " |")

    row("launch to ready, cold (ms)", lambda r: spread([x["ready"] for x in r["launches"]["cold"]]))
    row("launch to ready, warm (ms)", lambda r: spread([x["ready"] for x in r["launches"]["warm"]]))
    row("launch to bar mounted, warm (ms)", lambda r: spread([x["mounted"] for x in r["launches"]["warm"]]))
    for key, label in (("mem_idle", "idle, bar never shown"), ("mem_shown", "bar shown"),
                       ("mem_typed", "after typing session"), ("mem_graph", "after graph")):
        row(f"footprint, {label} (MB)", lambda r, k=key: spread([s[k]["footprint"] for s in r["sessions"]]))
        row(f"RSS, {label} (MB)", lambda r, k=key: spread([s[k]["rss"] for s in r["sessions"]]))
    row("processes", lambda r: str(len(r["sessions"][0]["mem_graph"]["processes"])))
    row("idle CPU, bar hidden (% of a core)", lambda r: spread([s["cpu_idle"] for s in r["sessions"]], "{:.2f}"))
    row("CPU while typing, 40 ms/key (% of a core)", lambda r: spread([s["cpu_typing"] for s in r["sessions"]], "{:.1f}"))
    row("keystroke to live answer, ms: median / p90 / p99 / max",
        lambda r: dist([m for s in r["sessions"] for m in s["keys"]["ms"]]))
    row("keystroke median per run (ms)", lambda r: spread([pct(s["keys"]["ms"], 50) for s in r["sessions"]], "{:.2f}"))
    row("keystroke timeouts (>10 s)", lambda r: str(sum(s["keys"].get("timeouts", 0) for s in r["sessions"])))
    for line in runs[names[0]]["sessions"][0]["heavy"]:
        row(f"`{line}` (ms)", lambda r, l=line: spread([m for s in r["sessions"] for m in s["heavy"][l]], "{:.1f}"))
    row("app on disk (MB)", lambda r: f"{r['size_mb']:.1f}")
    print("| | " + " | ".join(names) + " |")
    print("|---|" + "---|" * len(names))
    print("\n".join(rows))
    print()
    for n in names:
        procs = runs[n]["sessions"][0]["mem_graph"]["processes"]
        print(f"{n} after graph: " + ", ".join(f"{p['name']} {p['footprint'] or 0:.0f}/{p['rss']:.0f} MB" for p in procs))


if __name__ == "__main__":
    {"engine": engine, "app": app}[sys.argv[1]](sys.argv[2])
