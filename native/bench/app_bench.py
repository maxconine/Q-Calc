#!/usr/bin/env python3
"""Both Mac apps, measured the same way: launch, memory, CPU and typing latency.

  python3 native/bench/app_bench.py [--runs 3] [--warm 5] [--only html|native] [--out native/bench/results]

Builds first: `zsh macos/build.sh` (macos/dist/Q Calc.app) and `zsh native/mac/build.sh --arch=arm64`
(native/mac/dist/Q Calc Dev.app). Neither build is launched as is: each is copied under /tmp and given its own
bundle id, support folder and shortcut (Q Calc Bench, com.maxconine.qcalc.bench; Q Calc Native Bench,
com.maxconine.qcalc.native-bench), so an installed Q Calc or Q Calc Dev is never touched or quit. Every process
this starts is quit by pid, and the copies, their defaults and folders are removed at the end.

Driving: the native copy takes `bench:<spec>` over its dev command channel (Sources/App/DevBench.swift); the web
copy gets page_bench.js injected into its quick.html, which long-polls this script on 127.0.0.1 for the same specs.
"""
import argparse, json, os, plistlib, queue, re, shutil, signal, socket, subprocess, sys, tempfile, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
HOME = os.path.expanduser("~")
APPS = {
    "html": dict(src=f"{ROOT}/macos/dist/Q Calc.app", name="Q Calc Bench", id="com.maxconine.qcalc.bench",
                 hotkey="ctrl+alt+shift+cmd+j", folder="Q Calc"),
    # QCALC_BENCH_NATIVE_APP: another native build to measure (a saved copy, for A/B runs)
    "native": dict(src=os.environ.get("QCALC_BENCH_NATIVE_APP", f"{ROOT}/native/mac/dist/Q Calc Dev.app"), name="Q Calc Native Bench",
                   id="com.maxconine.qcalc.native-bench", hotkey="ctrl+alt+shift+cmd+k", folder="Q Calc Native Bench"),
}
GRAPH = "graph sin(1/x) + sin(40x)"
HEAVY_LINES = ["integral of x^2 sin(x) from 0 to pi", "integral of e^(-x^2) from -inf to inf",
               "solve({x^2 + y^2 = 4, x^2 - y = 2}, x = 1, y = 1)", "3 ft + 7 in + 25 cm + 1.2 m + 3 yd to mm"]

# ---- the page's channel ----
commands, replies = queue.Queue(), queue.Queue()
STARTED = []  # every app process this started, killed on the way out whatever happens
signal.signal(signal.SIGTERM, lambda *_: sys.exit(1))


class Channel(BaseHTTPRequestHandler):
    def log_message(self, fmt, *a):
        if os.environ.get("QCALC_BENCH_DEBUG"):
            print(time.strftime("%H:%M:%S"), fmt % a, file=sys.stderr, flush=True)

    def reply(self, body):
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path.startswith("/sleep"):
            time.sleep(int(self.path.split("ms=")[1]) / 1000)
            return self.reply({})
        try:
            self.reply(commands.get(timeout=45))
        except queue.Empty:
            self.reply({"noop": True})

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)) or 0)
        replies.put((self.path, json.loads(body or b"{}"), time.time() * 1000))
        self.reply({})


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# ---- process facts ----
def webkit_pids():
    out = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, text=True).stdout
    return {int(l.split()[0]) for l in out.splitlines() if "com.apple.WebKit." in l}


def alive(pids):
    out = subprocess.run(["ps", "-o", "pid=", "-p", ",".join(map(str, pids))], capture_output=True, text=True).stdout
    return [int(x) for x in out.split()]


def cpu_seconds(pids):
    out = subprocess.run(["ps", "-o", "pid=,time=", "-p", ",".join(map(str, pids))], capture_output=True, text=True).stdout
    total = 0.0
    for line in out.splitlines():
        t = line.split()[1]
        parts = [float(p) for p in t.split(":")]
        total += sum(v * 60 ** i for i, v in enumerate(reversed(parts)))
    return total


def age_s(etime):
    days, _, clock = etime.rpartition("-")
    parts = [int(p) for p in clock.split(":")]
    return int(days or 0) * 86400 + sum(v * 60 ** i for i, v in enumerate(reversed(parts)))


def to_mb(num, unit):
    return float(num) * {"B": 1 / 2**20, "KB": 1 / 1024, "MB": 1, "GB": 1024}[unit.upper()]


def memory(pids):
    """footprint (phys_footprint, what Activity Monitor calls Memory) and RSS, in MB, per process and summed"""
    each = {}
    for pid in pids:
        fp = subprocess.run(["footprint", "-p", str(pid)], capture_output=True, text=True).stdout
        m = re.search(r"phys_footprint:\s*([\d.]+)\s*([KMG]?B)", fp)
        rss = subprocess.run(["ps", "-o", "rss=,etime=,comm=", "-p", str(pid)], capture_output=True, text=True).stdout.split(None, 2)
        if not rss:
            continue
        each[pid] = dict(footprint=to_mb(*m.groups()) if m else None, rss=int(rss[0]) / 1024, age=age_s(rss[1]),
                         name=os.path.basename(rss[2].strip()))
    return dict(footprint=sum(v["footprint"] or 0 for v in each.values()), rss=sum(v["rss"] for v in each.values()),
                processes=list(each.values()))


# ---- the bench copies ----
def defaults(domain, *args):
    subprocess.run(["defaults", "write", domain, *args], check=True)


def prepare(kind, work, port):
    a = APPS[kind]
    app = f"{work}/{kind}/{a['name']}.app"
    os.makedirs(os.path.dirname(app))
    subprocess.run(["ditto", a["src"], app], check=True)
    plist = f"{app}/Contents/Info.plist"
    with open(plist, "rb") as f:
        info = plistlib.load(f)
    info.update(CFBundleIdentifier=a["id"], CFBundleName=a["name"], CFBundleDisplayName=a["name"],
                QCDefaultHotKey=a["hotkey"], SUEnableAutomaticChecks=False, SUAutomaticallyUpdate=False)
    if kind == "native":
        info["QCSupportFolder"] = a["folder"]
    else:
        info["NSAppTransportSecurity"] = {"NSAllowsLocalNetworking": True}
        web = f"{app}/Contents/Resources/web"
        with open(f"{HERE}/page_bench.js") as f:
            js = f.read().replace("__BENCH_PORT__", str(port))
        with open(f"{web}/bench.js", "w") as f:
            f.write(js)
        with open(f"{web}/quick.html") as f:
            html = f.read()
        with open(f"{web}/quick.html", "w") as f:
            f.write(html.replace("<head>", '<head>\n    <script src="./bench.js"></script>', 1))
    with open(plist, "wb") as f:
        plistlib.dump(info, f)
    # codesign refuses the Documents folder's provenance xattrs, as macos/build.sh notes
    subprocess.run(["xattr", "-cr", app], check=True)
    signed = subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", "--identifier", a["id"], app],
                            capture_output=True, text=True)
    if signed.returncode:
        sys.exit(f"codesign {app}: {signed.stderr}")
    d = a["id"]
    subprocess.run(["defaults", "delete", d], capture_output=True)
    defaults(d, "qcalc.firstRunDone", "-bool", "true")
    defaults(d, "qcalc.shareUsage", "-bool", "false")
    defaults(d, "qcalc.hotKeyNudges", "-int", "3")
    defaults(d, "qcalc.onboarding", "-dict", "opens", "-int", "9", "commits", "-int", "9",
             "hints", "-int", "2147483647", "done", "-int", "1")
    defaults(d, "SUEnableAutomaticChecks", "-bool", "false")
    return app


def cleanup(kind):
    a = APPS[kind]
    subprocess.run(["defaults", "delete", a["id"]], capture_output=True)
    if kind == "native":
        for d in (f"{HOME}/Library/Application Support/{a['folder']}", f"{HOME}/Library/Logs/{a['folder']}"):
            shutil.rmtree(d, ignore_errors=True)


class Running:
    """one launch of a bench copy, and everything it started"""

    def __init__(self, kind, app):
        self.kind, self.app, self.id = kind, app, APPS[kind]["id"]
        self.before = webkit_pids()
        while not replies.empty():
            replies.get()
        env = dict(os.environ, QCALC_LAUNCH_TIMING="1")
        exe = f"{app}/Contents/MacOS/QCalc"
        self.spawn = time.time() * 1000
        self.proc = subprocess.Popen([exe], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        STARTED.append(self.proc)
        self.lines = queue.Queue()
        threading.Thread(target=lambda: [self.lines.put(l) for l in self.proc.stderr], daemon=True).start()

    def ready(self, timeout=60):
        """ms from spawn to: the app's launch done, and its engine warm"""
        end = time.time() + timeout
        if self.kind == "html":
            while time.time() < end:
                path, body, _ = replies.get(timeout=max(0.1, end - time.time()))
                if path == "/ready":
                    return dict(mounted=body["mountedAt"] - self.spawn, ready=body["warmAt"] - self.spawn)
            raise TimeoutError
        launched = None
        log = f"{HOME}/Library/Logs/{APPS['native']['folder']}/overlay.log"
        while time.time() < end:
            try:
                line = self.lines.get(timeout=0.05)
                m = re.search(r"qcalc-launch-ms ([\d.]+)", line)
                if m:
                    launched = float(m.group(1))
            except queue.Empty:
                pass
            if launched is not None and os.path.exists(log):
                warm = re.findall(r"engine warm (\d+) ms after process start", open(log, errors="ignore").read())
                if warm and len(warm) > self.warm_seen:
                    return dict(mounted=launched, ready=max(launched, float(warm[-1])))
        raise TimeoutError

    warm_seen = 0

    def pids(self):
        return alive([self.proc.pid] + sorted(webkit_pids() - self.before))

    def command(self, cmd):
        if self.kind == "html":
            if cmd == "show":
                subprocess.run(["open", self.app], check=True)  # a reopen: the app toggles its bar
            return
        script = (f"ObjC.import('Foundation'); $.NSDistributedNotificationCenter.defaultCenter"
                  f".postNotificationNameObjectUserInfoDeliverImmediately({json.dumps(self.id + '.command')}, "
                  f"{json.dumps(cmd)}, $(), true)")
        subprocess.run(["osascript", "-l", "JavaScript", "-e", script], check=True, capture_output=True)

    def bench(self, spec, timeout=900):
        if self.kind == "html":
            commands.put(spec)
            end = time.time() + timeout
            while time.time() < end:
                path, body, _ = replies.get(timeout=max(0.1, end - time.time()))
                if path == "/result":
                    return body
            raise TimeoutError
        fd, out = tempfile.mkstemp(suffix=".json", prefix="qcalc-bench-out-")
        os.close(fd)
        os.unlink(out)
        spec_path = out.replace("-out-", "-spec-")
        with open(spec_path, "w") as f:
            json.dump(dict(spec, out=out), f)
        self.command(f"bench:{spec_path}")
        end = time.time() + timeout
        while not os.path.exists(out):
            if time.time() > end:
                raise TimeoutError
            time.sleep(0.1)
        time.sleep(0.1)
        with open(out) as f:
            body = json.load(f)
        os.unlink(out)
        os.unlink(spec_path)
        return body

    def quit(self):
        self.proc.send_signal(signal.SIGTERM)
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()


def native_log_count():
    log = f"{HOME}/Library/Logs/{APPS['native']['folder']}/overlay.log"
    if not os.path.exists(log):
        return 0
    return len(re.findall(r"engine warm \d+ ms", open(log, errors="ignore").read()))


def launch(kind, app):
    r = Running(kind, app)
    r.warm_seen = native_log_count() if kind == "native" else 0
    return r


def launch_runs(kind, app, work, cold, warm):
    out = dict(cold=[], warm=[])
    for i in range(cold):
        # a fresh copy each time: a new path and new files, never launched, so nothing is cached for it by path
        fresh = f"{work}/{kind}/cold{i}/{os.path.basename(app)}"
        subprocess.run(["ditto", app, fresh], check=True)
        if kind == "native":
            shutil.rmtree(f"{HOME}/Library/Logs/{APPS['native']['folder']}", ignore_errors=True)
        time.sleep(3)
        r = launch(kind, fresh)
        out["cold"].append(r.ready())
        time.sleep(1)
        r.quit()
        shutil.rmtree(os.path.dirname(fresh))
    for _ in range(warm):
        time.sleep(2)
        r = launch(kind, app)
        out["warm"].append(r.ready())
        time.sleep(1)
        r.quit()
    return out


def session(kind, app, inputs, idle_s):
    r = launch(kind, app)
    res = dict(launch=r.ready())
    time.sleep(10)
    res["mem_idle"] = memory(r.pids())
    pids = r.pids()
    c0, t0 = cpu_seconds(pids), time.time()
    time.sleep(idle_s)
    res["cpu_idle"] = 100 * (cpu_seconds(pids) - c0) / (time.time() - t0)
    r.command("show")
    time.sleep(3)
    res["mem_shown"] = memory(r.pids())
    typed = inputs[::13][:20]
    pids = r.pids()
    c0, t0 = cpu_seconds(pids), time.time()
    s = r.bench(dict(mode="session", inputs=typed, delayMs=40, enter=True, clear=False))
    res["cpu_typing"] = 100 * (cpu_seconds(r.pids()) - c0) / (time.time() - t0)
    res["typing_wall_ms"] = s.get("wallMs")
    time.sleep(3)
    res["mem_typed"] = memory(r.pids())
    # the peak too: drawing a graph can hold surfaces for a moment that are gone by the 3 s sample
    peak, stop = [], threading.Event()

    def sample():
        while not stop.is_set():
            peak.append(memory(r.pids())["footprint"])
            stop.wait(0.2)

    sampler = threading.Thread(target=sample, daemon=True)
    sampler.start()
    r.bench(dict(mode="session", inputs=[GRAPH], delayMs=40, enter=False, clear=False))
    time.sleep(3)
    stop.set()
    sampler.join()
    res["mem_graph"] = memory(r.pids())
    res["mem_graph_peak"] = max(peak) if peak else None
    keys = r.bench(dict(mode="keys", inputs=inputs))
    res["keys"] = keys
    if kind == "native":
        # the same inputs typed through the text view, a character at a time, as a person types them
        res["keys_field"] = r.bench(dict(mode="keys", inputs=inputs, field=True))
    heavy = r.bench(dict(mode="keys", inputs=HEAVY_LINES * 5, whole=True))
    res["heavy"] = {line: [m for m, i in zip(heavy["ms"], heavy["idx"]) if HEAVY_LINES[i % len(HEAVY_LINES)] == line]
                    for line in HEAVY_LINES}
    res["mem_end"] = memory(r.pids())
    r.quit()
    time.sleep(2)
    return res


def size_mb(path):
    out = subprocess.run(["du", "-sk", path], capture_output=True, text=True).stdout
    return int(out.split()[0]) / 1024


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--runs", type=int, default=3)
    p.add_argument("--cold", type=int, default=3)
    p.add_argument("--warm", type=int, default=5)
    p.add_argument("--idle", type=int, default=30)
    p.add_argument("--only", choices=list(APPS))
    p.add_argument("--out", default=f"{HERE}/results")
    args = p.parse_args()
    os.makedirs(args.out, exist_ok=True)
    for kind in APPS:
        if (not args.only or args.only == kind) and not os.path.isdir(APPS[kind]["src"]):
            sys.exit(f"missing {APPS[kind]['src']}: build it first")
    with open(f"{HERE}/inputs.json") as f:
        inputs = json.load(f)
    port = free_port()
    server = ThreadingHTTPServer(("127.0.0.1", port), Channel)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    work = tempfile.mkdtemp(prefix="qcalc-appbench.")
    try:
        for kind in APPS:
            if args.only and args.only != kind:
                continue
            app = prepare(kind, work, port)
            print(f"{kind}: launches…", flush=True)
            result = dict(size_mb=size_mb(APPS[kind]["src"]),
                          launches=launch_runs(kind, app, work, args.cold, args.warm), sessions=[])
            for i in range(args.runs):
                print(f"{kind}: session {i + 1}/{args.runs}…", flush=True)
                result["sessions"].append(session(kind, app, inputs, args.idle))
            with open(f"{args.out}/app-{kind}.json", "w") as f:
                json.dump(result, f)
            cleanup(kind)
    finally:
        for proc in STARTED:
            if proc.poll() is None:
                proc.kill()
        for kind in APPS:
            cleanup(kind)
        shutil.rmtree(work, ignore_errors=True)
        server.shutdown()
    subprocess.run([sys.executable, f"{HERE}/summarize.py", "app", args.out])


if __name__ == "__main__":
    main()
