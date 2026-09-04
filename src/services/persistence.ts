import type { Athlete, TestSession } from '../domain/models.ts';

export interface PersistedState {
  sessions: TestSession[];
  roster: Athlete[];
  volumeBoost: number;
  boostEnabled: boolean;
}

const fallbackKey = 'yoyo-tauri-state-v1';
const defaults: PersistedState = {
  sessions: [],
  roster: [],
  volumeBoost: 1,
  boostEnabled: true
};

function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function loadPersistedState(): Promise<PersistedState> {
  try {
    if (isTauri()) {
      const { load } = await import('@tauri-apps/plugin-store');
      const store = await load('yoyo-store.json', { autoSave: false, defaults: {} });
      return {
        sessions: (await store.get<TestSession[]>('sessions')) ?? [],
        roster: (await store.get<Athlete[]>('roster')) ?? [],
        volumeBoost: (await store.get<number>('volumeBoost')) ?? 1,
        boostEnabled: (await store.get<boolean>('boostEnabled')) ?? true
      };
    }

    const raw = localStorage.getItem(fallbackKey);
    return raw ? { ...defaults, ...JSON.parse(raw) } : defaults;
  } catch (error) {
    console.warn('Could not load persisted state', error);
    return defaults;
  }
}

export async function savePersistedState(state: PersistedState): Promise<void> {
  try {
    if (isTauri()) {
      const { load } = await import('@tauri-apps/plugin-store');
      const store = await load('yoyo-store.json', { autoSave: false, defaults: {} });
      await store.set('sessions', state.sessions);
      await store.set('roster', state.roster);
      await store.set('volumeBoost', state.volumeBoost);
      await store.set('boostEnabled', state.boostEnabled);
      await store.save();
      return;
    }
    localStorage.setItem(fallbackKey, JSON.stringify(state));
  } catch (error) {
    console.warn('Could not persist app state', error);
  }
}
