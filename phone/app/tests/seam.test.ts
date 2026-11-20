// The child's seam: reading what Kotlin will send, and the app's own host
// where no native side answers yet.

import { isSamsung, readChange, readMaker, readPairing, readRules, readSteps, readToday, SeamError, STEP_IDS } from '../src/childSeam';
import { childHost, HostError } from '../src/host';
import { ENGINE_OFF, offFrom, RULES, TODAY } from '../src/mock/childMock';

describe('reading steps', () => {
  it('is on only for the exact word "on"', () => {
    const steps = readSteps({ notifications: 'on', phone: 'on', usage: true, accessibility: 'ON', battery: 'yes', protection: 1 });
    expect(steps).toEqual({ notifications: 'on', phone: 'on', usage: 'off', accessibility: 'off', battery: 'off', protection: 'off' });
  });

  it('reads the phone step, and a missing phone step as off', () => {
    expect(readSteps({ phone: 'on' }).phone).toBe('on');
    expect(readSteps({ notifications: 'on', usage: 'on', accessibility: 'on', battery: 'on', protection: 'on' }).phone).toBe('off');
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

  it('keeps the bar between 0 and 1', () => {
    expect(readToday({ ...TODAY, usedPart: 1.7 }).usedPart).toBe(1);
    expect(readToday({ ...TODAY, usedPart: -0.2 }).usedPart).toBe(0);
  });

  it('reads the steps a guardian is needed for with the engine’s sentence for each, in setup order, each once', () => {
    const today = readToday({
      ...TODAY,
      guardianNeeded: {
        off: ['protection', 'phone', 'nonsense', 'phone'],
        words: ['Protection words.', 'Phone words.', 'Words for a step this app does not know.', 'Phone words again.'],
      },
    });
    expect(today.guardianNeeded).toEqual({ off: ['phone', 'protection'], words: ['Phone words.', 'Protection words.'] });
    expect(readToday({ ...TODAY, guardianNeeded: { off: [], words: [] } }).guardianNeeded).toBeNull();
    expect(readToday({ ...TODAY, guardianNeeded: null }).guardianNeeded).toBeNull();
    expect(readToday({ ...TODAY, guardianNeeded: { off: ['nonsense'], words: ['Unknown.'] } }).guardianNeeded).toBeNull();
  });

  it('refuses steps and sentences that do not line up', () => {
    const bad = (guardianNeeded: unknown) => () => readToday({ ...TODAY, guardianNeeded });
    expect(bad({ off: ['usage', 'battery'], words: ['Only one sentence.'] })).toThrow(/do not line up/);
    expect(bad({ off: ['usage'], words: ['One.', 'Two.'] })).toThrow(/do not line up/);
    expect(bad({ off: ['usage'] })).toThrow(/today\.guardianNeeded\.words/);
    expect(bad({ off: ['usage'], words: [3] })).toThrow(/today\.guardianNeeded\.words/);
    expect(bad({ off: 'usage', words: ['One.'] })).toThrow(/today\.guardianNeeded\.off/);
    expect(bad({ off: ['usage'], words: ['  '] })).toThrow(/today\.guardianNeeded\.words\[0\] is empty/);
    expect(bad('usage')).toThrow(SeamError);
  });

  it('reads what the mock’s engine says for every step off, unchanged', () => {
    const all = offFrom(Object.fromEntries(STEP_IDS.map((id) => [id, 'off'])) as never);
    expect(all).toEqual({ off: [...STEP_IDS], words: STEP_IDS.map((id) => ENGINE_OFF[id]) });
    expect(readToday(JSON.parse(JSON.stringify({ ...TODAY, guardianNeeded: all }))).guardianNeeded).toEqual(all);
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

describe('reading who made the phone', () => {
  it('passes Android’s own name through, and refuses what is not one', () => {
    expect(readMaker('samsung')).toBe('samsung');
    expect(readMaker(' Google ')).toBe('Google');
    expect(() => readMaker(null)).toThrow(SeamError);
    expect(() => readMaker({ maker: 'samsung' })).toThrow(SeamError);
  });

  it('knows a Samsung by Android’s name for it, whatever its case', () => {
    expect(isSamsung('samsung')).toBe(true);
    expect(isSamsung('Samsung')).toBe(true);
    expect(isSamsung(' SAMSUNG ')).toBe(true);
    expect(isSamsung('Google')).toBe(false);
    expect(isSamsung('')).toBe(false);
    expect(isSamsung('samsung electronics')).toBe(false);
  });
});

describe('the app’s host where no child module answers', () => {
  it('says the phone is not paired', async () => {
    await expect(childHost.pairing()).resolves.toEqual({ paired: false });
  });

  it('answers nothing else, and never with made up data', async () => {
    for (const ask of [childHost.agreed, childHost.steps, childHost.today, childHost.rules, childHost.introSeen, childHost.maker]) {
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
