// The child's seam: reading what Kotlin will send, and the app's own host
// where no native side answers yet.

import { readChange, readPairing, readRules, readSteps, readToday, SeamError, STEP_IDS } from '../src/childSeam';
import { childHost, HostError } from '../src/host';
import { RULES, TODAY } from '../src/mock/childMock';

describe('reading steps', () => {
  it('is on only for the exact word "on"', () => {
    const steps = readSteps({ notifications: 'on', usage: true, accessibility: 'ON', battery: 'yes', protection: 1 });
    expect(steps).toEqual({ notifications: 'on', usage: 'off', accessibility: 'off', battery: 'off', protection: 'off' });
  });

  it('reads a missing step as off', () => {
    expect(readSteps({})).toEqual(Object.fromEntries(STEP_IDS.map((id) => [id, 'off'])));
  });

  it('refuses what is not an object', () => {
    expect(() => readSteps('on')).toThrow(SeamError);
    expect(() => readSteps(null)).toThrow(SeamError);
  });
});

describe('reading the pairing', () => {
  it('is paired only with true and a name', () => {
    expect(readPairing({ paired: true, childName: ' Asha ' })).toEqual({ paired: true, childName: 'Asha' });
    expect(readPairing({ paired: 'true', childName: 'Asha' })).toEqual({ paired: false });
    expect(readPairing({ paired: false })).toEqual({ paired: false });
    expect(() => readPairing({ paired: true, childName: '  ' })).toThrow(SeamError);
    expect(() => readPairing({ paired: true })).toThrow(SeamError);
  });
});

describe('reading Today and the rules', () => {
  it('reads what the mock holds, unchanged', () => {
    expect(readToday(JSON.parse(JSON.stringify(TODAY)))).toEqual(TODAY);
    expect(readRules(JSON.parse(JSON.stringify(RULES)))).toEqual(RULES);
  });

  it('keeps the bar between 0 and 1, and the steps a guardian is needed for in setup order, each once', () => {
    const today = readToday({ ...TODAY, usedPart: 1.7, guardianNeeded: { off: ['protection', 'usage', 'nonsense', 'usage'] } });
    expect(today.usedPart).toBe(1);
    expect(today.guardianNeeded).toEqual({ off: ['usage', 'protection'] });
    expect(readToday({ ...TODAY, guardianNeeded: { off: [] } }).guardianNeeded).toBeNull();
  });

  it('refuses a wrong shape with a message that says where', () => {
    expect(() => readToday({ ...TODAY, left: { kind: 'counting', words: '1 h' } })).toThrow(/today\.left\.kind/);
    expect(() => readToday({ ...TODAY, apps: [{ name: 'Maps' }] })).toThrow(/today\.apps\[0\]\.minutes/);
    expect(() => readRules({ ...RULES, alwaysOpen: 'Phone' })).toThrow(/rules\.alwaysOpen/);
  });

  it('reads a change event, or nothing', () => {
    expect(readChange({ what: 'steps' })).toBe('steps');
    expect(readChange({ what: 'everything' })).toBeNull();
    expect(readChange('steps')).toBeNull();
  });
});

describe('the app’s host where no child module answers', () => {
  it('says the phone is not paired', async () => {
    await expect(childHost.pairing()).resolves.toEqual({ paired: false });
  });

  it('answers nothing else, and never with made up data', async () => {
    for (const ask of [childHost.agreed, childHost.steps, childHost.today, childHost.rules, childHost.introSeen]) {
      await expect(ask()).rejects.toBeInstanceOf(HostError);
    }
    await expect(childHost.openStep('usage')).rejects.toBeInstanceOf(HostError);
    await expect(childHost.askForMoreTime()).rejects.toBeInstanceOf(HostError);
  });

  it('has nothing to subscribe to, and stopping is safe', () => {
    const stop = childHost.subscribe(() => undefined);
    expect(() => stop()).not.toThrow();
  });
});
