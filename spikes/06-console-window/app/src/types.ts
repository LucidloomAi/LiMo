// The shapes the Rust side (src-tauri/src/family.rs) sends back, as JSON.
// The mock in host.ts answers with the same shapes.

export type Device = {
  id: string;
  name: string;
  kind: string;
  locked: boolean;
};

export type ChildToday = {
  id: string;
  name: string;
  usedMinutes: number;
  leftMinutes: number;
  locked: boolean;
  devices: Device[];
};

export type FamilyView = {
  children: ChildToday[];
  // Which side answered: "rust" from the Tauri shell, "mock" anywhere else.
  answeredBy: string;
  // Increases by one on every change, so a screen can tell a new answer from an old one.
  revision: number;
};
