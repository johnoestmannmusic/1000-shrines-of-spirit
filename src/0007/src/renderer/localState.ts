export interface LocalState {
  mutes: boolean[];
  instrumentNames: string[];
  instrumentColors: Array<[number, number, number]>;
  reference: boolean;
}

const KEY = "lantern-local-state-v1";

export function loadLocalState(): LocalState | null {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<LocalState>;
    return {
      mutes: Array.isArray(parsed.mutes) ? parsed.mutes.map(Boolean) : [],
      instrumentNames: Array.isArray(parsed.instrumentNames)
        ? parsed.instrumentNames.map(String)
        : [],
      instrumentColors: Array.isArray(parsed.instrumentColors)
        ? parsed.instrumentColors.map((c) =>
            Array.isArray(c) && c.length === 3 ? ([c[0], c[1], c[2]] as [number, number, number]) : [0, 0, 0],
          )
        : [],
      reference: !!parsed.reference,
    };
  } catch {
    return null;
  }
}

export function saveLocalState(state: LocalState): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(state));
  } catch {
    /* storage unavailable */
  }
}
