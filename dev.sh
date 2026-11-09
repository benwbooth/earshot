#!/usr/bin/env bash
# Rebuilds and restarts Earshot every time you save a change.
#   ./dev.sh          the Qt app, via devloop (~/src/home-services/devloop):
#                     the same optimized build the Earshot launcher runs
#   ./dev.sh web      the web version: rebuilds the interface, restarts its server
#   ./dev.sh all      both at once
# Extra arguments after web/all go to watchexec (e.g. ./dev.sh web --clear).
#
# Only one copy runs at a time: starting dev.sh stops an earlier dev.sh, any
# running Earshot app, and any earlier dev web server first. The dev app then
# owns Earshot's "only one at a time" check, so opening Earshot again just
# raises its window. It uses your real library; set EARSHOT_DATA_DIR to use
# another one (that also lets it run next to another Earshot). The dev web
# server listens on 127.0.0.1:8791, so it doesn't clash with the app's own web
# version on 8790; set EARSHOT_LISTEN to change it.
set -euo pipefail
root=$(cd "$(dirname "$0")" && pwd)
cd "$root"
devloop="$HOME/src/home-services/devloop/devloop"

# The app alone is devloop's job.
if [ "${1:-app}" = app ]; then
  [ $# -gt 0 ] && shift
  exec "$devloop" dev earshot "$@"
fi

# Everything (Qt, watchexec, the wasm tools) comes from the flake's dev shell.
if [ -z "${IN_NIX_SHELL:-}" ]; then
  exec nix develop "$root" --command "$root/dev.sh" "$@"
fi

mode=${1:-app}
[ $# -gt 0 ] && shift

case "$mode" in -h|--help|help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit ;; esac

# A process and everything it started.
tree() { local c; echo "$1"; for c in $(pgrep -P "$1"); do tree "$c"; done; }

# Stops the given processes (and what they started), waiting up to 5s.
stop() {
  local pids=() p
  for p in "$@"; do pids+=($(tree "$p")); done
  [ ${#pids[@]} -gt 0 ] || return 0
  kill -TERM "${pids[@]}" 2>/dev/null || true
  for _ in $(seq 50); do kill -0 "${pids[@]}" 2>/dev/null || return 0; sleep 0.1; done
  kill -KILL "${pids[@]}" 2>/dev/null || true
}

# Earlier dev.sh runs from this folder (not this one, nor the nix develop that
# started it).
# A run is bash running dev.sh, or the nix develop that runs it; a shell or
# editor that merely mentions dev.sh doesn't count.
is_run() {
  local args=()
  mapfile -d '' args < "/proc/$1/cmdline" 2>/dev/null || return 1
  case "${args[0]##*/}" in
    bash|sh) case "${args[1]:-}" in dev.sh|*/dev.sh) return 0 ;; esac ;;
    nix) [[ " ${args[*]} " == *" --command $root/dev.sh "* ]] && return 0 ;;
  esac
  return 1
}
old=()
for p in $(pgrep -u "$USER" -f 'dev\.sh'); do
  if [ "$p" != $$ ] && [ "$p" != "$PPID" ] && is_run "$p" \
     && [ "$(readlink "/proc/$p/cwd" 2>/dev/null)" = "$root" ]; then
    old+=("$p")
  fi
done
# Running Earshot apps: an installed one, or a dev build.
if [ "$mode" != web ]; then
  old+=($(pgrep -u "$USER" -x 'earshot|\.earshot-wrapp' || true))
fi
# Dev web servers left from this folder (not an installed earshot-server).
if [ "$mode" != app ]; then
  for p in $(pgrep -u "$USER" -x earshot-server || true); do
    case "$(readlink "/proc/$p/exe" 2>/dev/null)" in "$root"/target/*) old+=("$p") ;; esac
  done
fi
if [ ${#old[@]} -gt 0 ]; then
  echo "Stopping what was already running: ${old[*]}"
  stop "${old[@]}"
fi

export EARSHOT_LISTEN=${EARSHOT_LISTEN:-127.0.0.1:8791}

common=(--restart --debounce 500ms --stop-signal SIGTERM --stop-timeout 5s
        --ignore 'target/**' --ignore 'result*')

watch_app() { "$devloop" dev earshot; }

watch_web() {
  watchexec "${common[@]}" \
    -w crates -w qml/icons -w scripts/build-web.sh -w Cargo.toml -w Cargo.lock \
    -e rs,toml,lock,css,html,webmanifest,svg,sh \
    "$@" -- bash -c 'scripts/build-web.sh target/web && echo "Web version at http://$EARSHOT_LISTEN/" && EARSHOT_WEB_DIR=target/web exec cargo run -p earshot-server'
}

case "$mode" in
  app) watch_app "$@" ;;
  web) watch_web "$@" ;;
  all)
    trap 'kill 0' EXIT INT TERM
    watch_web "$@" &
    watch_app "$@" &
    wait
    ;;
  *) echo "usage: ./dev.sh [app|web|all] [watchexec options]" >&2; exit 2 ;;
esac
