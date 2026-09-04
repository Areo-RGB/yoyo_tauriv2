/**
 * Re-applies our custom AndroidManifest.xml and MainActivity.kt tweaks to the
 * generated Tauri Android project (`src-tauri/gen/android`, created by `tauri android init`).
 *
 * The gen/ tree is regenerated from templates on `init`, which wipes hand edits.
 * This script is idempotent and runs automatically as part of the `android:init`,
 * `android:dev` and `android:build` npm scripts.
 *
 * Current tweaks:
 * - `windowLayoutInDisplayCutoutMode="shortEdges"` on MainActivity in AndroidManifest.xml
 * - Edge-to-edge fullscreen + sticky immersive system UI hiding in MainActivity.kt
 *
 * Usage: `node scripts/patch-android-manifest.mjs` (extra args ignored).
 */
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const manifestPath = path.join(
  root,
  'src-tauri/gen/android/app/src/main/AndroidManifest.xml'
);
const mainActivityPath = path.join(
  root,
  'src-tauri/gen/android/app/src/main/java/com/aistudio/yoyoir1/track/MainActivity.kt'
);

const ATTR = 'android:windowLayoutInDisplayCutoutMode="shortEdges"';

if (!existsSync(manifestPath)) {
  console.log('[patch-manifest] gen/android not present (init not run yet) — skipping.');
  process.exit(0);
}

// 1. Patch AndroidManifest.xml
let xml = readFileSync(manifestPath, 'utf8');
const activityRe = /(<activity\b[^<>]*android:name="\.MainActivity"[^<>]*)(\/?>)/;
const m = xml.match(activityRe);
if (m && !m[1].includes('windowLayoutInDisplayCutoutMode')) {
  xml = xml.replace(activityRe, `$1\n            ${ATTR}$2`);
  writeFileSync(manifestPath, xml);
  console.log(`[patch-manifest] applied ${ATTR} to MainActivity in AndroidManifest.xml.`);
}

// 2. Patch MainActivity.kt for Edge-to-Edge immersive fullscreen
if (existsSync(mainActivityPath)) {
  let kt = readFileSync(mainActivityPath, 'utf8');
  if (!kt.includes('hideSystemUI')) {
    const fullKt = `package com.aistudio.yoyoir1.track

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    hideSystemUI()
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) {
      hideSystemUI()
    }
  }

  private fun hideSystemUI() {
    WindowCompat.setDecorFitsSystemWindows(window, false)
    val controller = WindowCompat.getInsetsController(window, window.decorView)
    controller.systemBarsBehavior =
      WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    controller.hide(WindowInsetsCompat.Type.systemBars())
  }
}
`;
    writeFileSync(mainActivityPath, fullKt);
    console.log('[patch-manifest] applied hideSystemUI() immersive mode to MainActivity.kt.');
  }
}
