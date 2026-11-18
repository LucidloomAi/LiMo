// The one seam between the screens and whatever runs them.
//
// Screens call host.request(name, args) and nothing else. Inside the Tauri
// window the call goes to a Rust command through Tauri's invoke. Anywhere else
// (a normal browser with `expo start --web`, the web export served as files,
// a phone) it is answered by the small mock at the bottom of this file.
//
// How the running place is found: Tauri v2 puts `__TAURI_INTERNALS__` on the
// page's global object before any of the page's own scripts run. Nothing else
// does, so its presence means "inside the Tauri window".

import { invoke } from '@tauri-apps/api/core';

import type { ChildToday, FamilyView } from './types';

export type Where = 'tauri' | 'mock';

export type Args = Record<string, unknown>;

function where(): Where {
  return typeof globalThis === 'object' && globalThis !== null && '__TAURI_INTERNALS__' in globalThis
    ? 'tauri'
    : 'mock';
}

async function request<T>(name: string, args: Args = {}): Promise<T> {
  if (where() === 'tauri') {
    return invoke<T>(name, args);
  }
  return mockRequest(name, args) as T;
}

export const host = { where, request };

// ---------------------------------------------------------------------------
// The mock: same commands, same shapes, same rules as the Rust side, held in
// this page's memory. It forgets everything on reload.

type MockChild = Omit<ChildToday, 'leftMinutes'> & { budgetMinutes: number; extraMinutes: number };

const mockChildren: MockChild[] = [
  {
    id: 'c1',
    name: 'Asha',
    usedMinutes: 45,
    budgetMinutes: 120,
    extraMinutes: 0,
    locked: false,
    devices: [
      { id: 'd1', name: 'Tablet', kind: 'Android', locked: false },
      { id: 'd2', name: 'Family PC, Asha', kind: 'Windows account', locked: false },
    ],
  },
  {
    id: 'c2',
    name: 'Kabir',
    usedMinutes: 80,
    budgetMinutes: 90,
    extraMinutes: 0,
    locked: false,
    devices: [
      { id: 'd3', name: 'Phone', kind: 'Android', locked: false },
      { id: 'd4', name: 'Family PC, Kabir', kind: 'Windows account', locked: false },
    ],
  },
];
let mockRevision = 1;

function mockView(): FamilyView {
  return {
    answeredBy: 'mock',
    revision: mockRevision,
    children: mockChildren.map((c) => ({
      id: c.id,
      name: c.name,
      usedMinutes: c.usedMinutes,
      leftMinutes: Math.max(0, c.budgetMinutes + c.extraMinutes - c.usedMinutes),
      locked: c.locked,
      devices: c.devices.map((d) => ({ ...d })),
    })),
  };
}

function mockChild(args: Args): MockChild {
  const child = mockChildren.find((c) => c.id === args.childId);
  if (!child) {
    throw new Error(`no child with id ${String(args.childId)}`);
  }
  return child;
}

function mockRequest(name: string, args: Args): unknown {
  switch (name) {
    case 'family_view':
      return mockView();
    case 'give_minutes': {
      const minutes = Number(args.minutes);
      if (!Number.isInteger(minutes) || minutes < 1 || minutes > 240) {
        throw new Error('minutes must be a whole number from 1 to 240');
      }
      const child = mockChild(args);
      child.extraMinutes += minutes;
      child.locked = false;
      child.devices.forEach((d) => (d.locked = false));
      mockRevision += 1;
      return mockView();
    }
    case 'lock_child': {
      const child = mockChild(args);
      child.locked = true;
      child.devices.forEach((d) => (d.locked = true));
      mockRevision += 1;
      return mockView();
    }
    default:
      throw new Error(`the mock has no command ${name}`);
  }
}
