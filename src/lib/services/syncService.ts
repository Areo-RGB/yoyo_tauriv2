import { invoke } from '@tauri-apps/api/core';
import { get } from 'svelte/store';
import {
  athletes,
  eliminateAthlete,
  markAthleteMiss,
  runtime,
  testState
} from '../state/testStore.ts';

export interface SyncStatus {
  isServerRunning: boolean;
  serverUrl: string | null;
  localIps: string[];
}

let syncTimer: number | null = null;
let serverRunning = false;
let currentServerUrl: string | null = null;

export async function getHostLocalIps(): Promise<string[]> {
  try {
    return await invoke<string[]>('get_local_ips');
  } catch (err) {
    console.warn('Failed to fetch local IPs from Tauri:', err);
    return ['127.0.0.1'];
  }
}

export async function startHostSyncServer(port = 8080): Promise<string> {
  try {
    const url = await invoke<string>('start_sync_server', { port });
    serverRunning = true;
    currentServerUrl = url;

    startSyncLoop();
    return url;
  } catch (err) {
    console.error('Failed to start host sync server:', err);
    throw err;
  }
}

export async function stopHostSyncServer(): Promise<void> {
  try {
    await invoke('stop_sync_server');
    serverRunning = false;
    currentServerUrl = null;
    if (syncTimer !== null) {
      clearInterval(syncTimer);
      syncTimer = null;
    }
  } catch (err) {
    console.error('Failed to stop host sync server:', err);
  }
}

function startSyncLoop() {
  if (syncTimer !== null) return;

  syncTimer = window.setInterval(async () => {
    if (!serverRunning) return;

    const rt = get(runtime);
    const payload = {
      status: get(testState),
      level: rt.shuttle?.levelDisplay ?? '1',
      shuttle: rt.shuttle?.shuttleNumber ?? 1,
      distance: rt.currentDistanceMeters ?? 0,
      athletes: get(athletes)
    };

    try {
      await invoke('broadcast_test_state', {
        stateJson: JSON.stringify(payload)
      });

      const actions = await invoke<string[]>('pop_remote_actions');
      for (const actionRaw of actions) {
        try {
          const parsed = JSON.parse(actionRaw);
          if (parsed.action === 'mark_miss' && parsed.athleteId) {
            markAthleteMiss(parsed.athleteId);
          } else if (parsed.action === 'eliminate' && parsed.athleteId) {
            eliminateAthlete(parsed.athleteId);
          }
        } catch {
          // invalid payload ignored
        }
      }
    } catch (e) {
      console.warn('Sync loop error:', e);
    }
  }, 250);
}
