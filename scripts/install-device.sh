#!/usr/bin/env bash
#
# Builds the Android APK, installs it on connected device(s), stops any prior
# instance of the app, and launches it.
#
# Usage:
#   scripts/install-device.sh [--release] [--no-build] [--device SERIAL] [--apk PATH]
#
#   --release        build a release APK instead of debug (needs signing set up)
#   --no-build       skip the build, install an existing APK
#   --device SERIAL  target one device (repeatable); default is all connected
#   --apk PATH       install this APK instead of auto-detecting the build output
#
# Examples:
#   scripts/install-device.sh
#   scripts/install-device.sh --no-build --device 192.168.1.50:5555
#
set -euo pipefail

APP_ID="com.aistudio.yoyoir1.track"
ACTIVITY="$APP_ID/.MainActivity"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APK_DIR="$ROOT/src-tauri/gen/android/app/build/outputs/apk"

MODE="debug"
BUILD=1
APK=""
DEVICES=()

log() { printf '\033[1;34m[install-device]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[install-device]\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m[install-device]\033[0m %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<'EOF'
Builds the Android APK, installs it on connected device(s), stops any prior
instance of the app, and launches it.

Usage:
  scripts/install-device.sh [--release] [--no-build] [--device SERIAL] [--apk PATH]
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) MODE="release"; shift ;;
    --no-build) BUILD=0; shift ;;
    --device)
      [[ $# -ge 2 ]] || die "--device needs a serial argument"
      DEVICES+=("$2"); shift 2 ;;
    --apk)
      [[ $# -ge 2 ]] || die "--apk needs a path argument"
      APK="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

# Ensure compatible JDK for Gradle (Gradle 8.14 requires Java <= 23)
if [[ -z "${JAVA_HOME:-}" ]] || "${JAVA_HOME}/bin/java" -version 2>&1 | grep -qE 'version "(2[4-9]|[3-9][0-9])'; then
  for candidate in "/home/paul/tools/jdk-21.0.12.1+1" "/usr/lib/jvm/java-21-openjdk" "/usr/lib/jvm/java-17-openjdk"; do
    if [[ -d "$candidate" ]]; then
      export JAVA_HOME="$candidate"
      export PATH="$JAVA_HOME/bin:$PATH"
      break
    fi
  done
fi

# Avoid conflict in Android Gradle Plugin when both ANDROID_PREFS_ROOT and ANDROID_USER_HOME are set
unset ANDROID_PREFS_ROOT

command -v adb >/dev/null 2>&1 || die "adb not found on PATH (install Android platform tools first)"
command -v npm >/dev/null 2>&1 || die "npm not found on PATH"

if [[ "$BUILD" == "1" ]]; then
  log "building $MODE APK..."
  if [[ "$MODE" == "release" ]]; then
    (cd "$ROOT" && npm run android:build -- --release)
    (cd "$ROOT" && npm run android:build)
  else
    (cd "$ROOT" && npm run android:build)
    (cd "$ROOT" && npm run android:build -- --debug)
  fi
fi

if [[ -z "$APK" ]]; then
  preferred="$APK_DIR/universal/$MODE/app-universal-$MODE.apk"
  if [[ "$MODE" == "debug" ]]; then
    preferred="$APK_DIR/universal/debug/app-universal-debug.apk"
  else
    preferred="$APK_DIR/universal/release/app-universal-release-unsigned.apk"
    [[ -f "$preferred" ]] || preferred="$APK_DIR/universal/release/app-universal-release.apk"
  fi

  if [[ -f "$preferred" ]]; then
    APK="$preferred"
  else
    APK="$(find "$APK_DIR" -name '*.apk' -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2- || true)"
  fi
fi
[[ -n "$APK" && -f "$APK" ]] || die "no APK found (build one first or pass --apk PATH)"
log "APK: $APK"

if [[ "${#DEVICES[@]}" -eq 0 ]]; then
  mapfile -t DEVICES < <(adb devices | awk '$2 == "device" { print $1 }')
  while read -r line; do warn "skipping $line"; done < <(adb devices | awk '$2 != "device" && $2 != "" && NR > 1 { print $0 }')
fi
[[ "${#DEVICES[@]}" -gt 0 ]] || die "no connected devices (plug one in / adb connect, then retry)"

for serial in "${DEVICES[@]}"; do
  log "[$serial] stopping prior instance..."
  adb -s "$serial" shell am force-stop "$APP_ID" \
    || warn "[$serial] force-stop failed (app may not be installed yet)"
  log "[$serial] installing (reinstall, keep data)..."
  adb -s "$serial" install -r "$APK"
  log "[$serial] launching $ACTIVITY..."
  adb -s "$serial" shell am start -n "$ACTIVITY"
done

log "done."
