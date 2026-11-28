#!/bin/sh
# Drive an isolated copy of Realtime Tex Desktop for verification. See SKILL.md.
# State of the run in progress: $RTD_STATE (default /tmp/rtd-verify/current).
set -u
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
APP="$REPO/target/release/Realtime Tex Desktop.app/Contents/MacOS/realtime-tex-desktop"
STATE=${RTD_STATE:-/tmp/rtd-verify/current}

die() { echo "rtd: $*" >&2; exit 1; }
load() {
  [ -f "$STATE" ] || die "no run in progress (rtd.sh launch first)"
  . "$STATE"
}
orc() { orca computer "$@" --app "pid:$PID" --json; }
ok() { python3 -c 'import json,sys; r=json.load(sys.stdin); sys.exit(0 if r.get("ok") else (print(json.dumps(r.get("error")),file=sys.stderr) or 1))'; }
win() { [ -n "${1:-}" ] && echo "$1" || echo "$WIN"; }
act() { # act <window|""> <orca action args...>: background input, no screenshot
  # RTD_RESTORE=1 brings the window forward first (it is on another Space); only with the user's OK
  w=$(win "$1"); shift
  orc "$@" --window-id "$w" --no-screenshot ${RTD_RESTORE:+--restore-window} | ok
}

cmd=${1:-help}; shift 2>/dev/null || true
case "$cmd" in
launch)
  # launch [--build] [--fresh-tour] [project-folder-or-.tex]
  build=0; tour=0; target=""
  for a in "$@"; do
    case "$a" in --build) build=1 ;; --fresh-tour) tour=1 ;; *) target="$a" ;; esac
  done
  [ -f "$STATE" ] && die "a run is in progress ($STATE); rtd.sh stop first"
  [ $build = 1 ] && { "$REPO/scripts/bundle.sh" >/dev/null || die "bundle.sh failed"; }
  [ -x "$APP" ] || die "no bundle at $APP (run with --build)"
  RUN=$(date +%Y%m%d-%H%M%S)
  ROOT=/tmp/rtd-verify/$RUN
  HOMEDIR=$ROOT/home
  EVID=$REPO/target/verify/$RUN
  SUP="$HOMEDIR/Library/Application Support/Realtime Tex Desktop"
  mkdir -p "$SUP" "$EVID"
  # the first launch starts the tour unless the settings say it was seen
  [ $tour = 1 ] || echo '{"tour_seen": true}' > "$SUP/settings.json"
  # rtex: reuse the pinned copy if the real support dir has it (else the app downloads it)
  real="$HOME/Library/Application Support/Realtime Tex Desktop/rtex"
  [ -d "$real" ] && cp -R "$real" "$SUP/rtex"
  if [ -n "$target" ]; then
    HOME=$HOMEDIR "$APP" "$target" >"$ROOT/app.log" 2>&1 &
  else
    HOME=$HOMEDIR "$APP" >"$ROOT/app.log" 2>&1 &
  fi
  PID=$!
  mkdir -p "$(dirname "$STATE")"
  printf 'RUN=%s\nROOT=%s\nHOMEDIR=%s\nEVID=%s\nPID=%s\nWIN=\n' "$RUN" "$ROOT" "$HOMEDIR" "$EVID" "$PID" > "$STATE"
  # ready = a window the app drew (a window can exist before orca can read it)
  i=0
  while [ $i -lt 60 ]; do
    W=$(orca computer list-windows --app "pid:$PID" --json 2>/dev/null |
      python3 -c 'import json,sys
try: w=json.load(sys.stdin)["result"]["windows"]; print(w[0]["id"] if w else "")
except Exception: print("")')
    [ -n "$W" ] && break
    kill -0 "$PID" 2>/dev/null || die "app exited; see $ROOT/app.log"
    sleep 0.5; i=$((i+1))
  done
  [ -n "$W" ] || die "no window after 30 s; see $ROOT/app.log"
  sed -i '' "s/^WIN=.*/WIN=$W/" "$STATE"
  sleep 1.5
  echo "run $RUN pid $PID window $W"
  echo "evidence: $EVID"
  "$0" windows
  ;;
doctor)
  load
  fail=0
  kill -0 "$PID" 2>/dev/null && echo "ok   process $PID alive" || { echo "FAIL process $PID gone"; fail=1; }
  ps -p "$PID" -o command= 2>/dev/null | grep -q "target/release/Realtime Tex Desktop.app" &&
    echo "ok   process is this repo's bundle" || { echo "FAIL process is not this repo's bundle"; fail=1; }
  ps -E -p "$PID" -o command= 2>/dev/null | grep -q "HOME=$HOMEDIR" &&
    echo "ok   HOME is the run's own ($HOMEDIR)" || { echo "FAIL HOME is not the run's own: refusing to drive"; fail=1; }
  bin=$(stat -f %m "$APP"); src=$(git -C "$REPO" log -1 --format=%ct)
  dirty=$(git -C "$REPO" status --porcelain -- src | head -1)
  [ "$bin" -ge "$src" ] && echo "ok   bundle newer than HEAD" || echo "WARN bundle older than HEAD (launch --build)"
  [ -n "$dirty" ] && echo "WARN src has uncommitted changes; the bundle may predate them"
  orca computer permissions --json | python3 -c 'import json,sys
p={x["id"]:x["status"] for x in json.load(sys.stdin)["result"]["permissions"]}
print(("ok  " if all(v=="granted" for v in p.values()) else "FAIL")+" orca permissions "+str(p))'
  if orc get-app-state --window-id "$WIN" --no-screenshot >/dev/null 2>&1 &&
     orc get-app-state --window-id "$WIN" --no-screenshot | ok 2>/dev/null; then
    echo "ok   window $WIN readable"
  else
    echo "FAIL window $WIN not readable: screen locked, or the window is gone (rtd.sh windows)"; fail=1
  fi
  exit $fail
  ;;
windows)
  load
  orca computer list-windows --app "pid:$PID" --json | python3 -c 'import json,sys
for w in json.load(sys.stdin)["result"]["windows"]:
    print(w["id"], repr(w["title"]), "%dx%d" % (w["width"], w["height"]))'
  ;;
use)
  load; [ -n "${1:-}" ] || die "use <window-id>"
  sed -i '' "s/^WIN=.*/WIN=$1/" "$STATE"; echo "window $1"
  ;;
shot)
  # shot <name> [window]: the full screenshot and a 1200 px wide copy, in the evidence dir
  load; name=${1:?shot <name> [window]}; w=$(win "${2:-}")
  f=$(orc get-app-state --window-id "$w" | python3 -c 'import json,sys
r=json.load(sys.stdin)
if not r.get("ok"): sys.exit("rtd: "+json.dumps(r.get("error")))
print(r["result"]["screenshot"]["path"])') || exit 1
  cp "$f" "$EVID/$name.png"
  sips -Z 1200 "$EVID/$name.png" --out "$EVID/$name.small.png" >/dev/null
  ww=$(orca computer list-windows --app "pid:$PID" --json | python3 -c "import json,sys
print([x['width'] for x in json.load(sys.stdin)['result']['windows'] if x['id']==$w][0])")
  sw=$(sips -g pixelWidth "$EVID/$name.small.png" | awk '/pixelWidth/{print $2}')
  echo "$EVID/$name.small.png"
  echo "window points = small-copy pixels x $(python3 -c "print(round($ww/$sw,4))")"
  ;;
click)  load; act "${3:-}" click --x "${1:?click x y [window]}" --y "$2" ;;
dclick) load; act "${3:-}" click --x "${1:?dclick x y [window]}" --y "$2" --click-count 2 ;;
tclick) load; act "${3:-}" click --x "${1:?tclick x y [window]}" --y "$2" --click-count 3 ;;
sclick) load; act "${3:-}" click --x "${1:?sclick x y [window]}" --y "$2" --modifiers shift ;;
rclick) load; act "${3:-}" click --x "${1:?rclick x y [window]}" --y "$2" --mouse-button right ;;
drag)   load; act "${5:-}" drag --from-x "${1:?drag x1 y1 x2 y2 [window]}" --from-y "$2" --to-x "$3" --to-y "$4" ;;
scroll) load; act "${5:-}" scroll --x "${1:?scroll x y up|down [pages] [window]}" --y "$2" --direction "${3:?up|down}" --pages "${4:-3}" ;;
key)    load; act "${2:-}" press-key --key "${1:?key <Key> [window]}" ;;
hotkey) load; act "${2:-}" hotkey --key "${1:?hotkey <Chord> [window]}" ;;
type)   load; act "${2:-}" type-text --text "${1:?type <text> [window]}" ;;
ax)
  # ax <query> [window]: accessibility elements matching text (native panels only; GPUI has none)
  load; w=$(win "${2:-}")
  orc get-app-state --window-id "$w" --query "${1:?ax <query> [window]}" --no-screenshot |
    python3 -c 'import json,sys; r=json.load(sys.stdin); print(r["result"]["snapshot"]["treeText"] if r.get("ok") else r["error"])'
  ;;
axclick) load; act "${2:?axclick <index> <window>}" click --element-index "$1" ;;
axset)   load; act "${3:?axset <index> <value> <window>}" set-value --element-index "$1" --value "$2" ;;
home)    load; echo "$HOMEDIR" ;;
settings) load; cat "$HOMEDIR/Library/Application Support/Realtime Tex Desktop/settings.json" ;;
evidence) load; echo "$EVID" ;;
stop)
  # stop: kill the run's own process, remove its HOME and scratch; the evidence stays
  load
  if kill -0 "$PID" 2>/dev/null && ps -E -p "$PID" -o command= | grep -q "HOME=$HOMEDIR"; then
    kill "$PID"; sleep 1; kill -0 "$PID" 2>/dev/null && kill -9 "$PID"
  fi
  rm -rf "$ROOT"; rm -f "$STATE"
  echo "stopped run $RUN; evidence kept in $EVID"
  ls "$EVID"
  ;;
*)
  sed -n '1,4p' "$0"; echo "commands: launch doctor windows use shot click dclick tclick sclick rclick drag scroll key hotkey type ax axclick axset home settings evidence stop"
  ;;
esac
