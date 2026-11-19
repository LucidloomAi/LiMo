// The one seam between the screens and whatever runs them, as in proof six
// (spikes/06-console-window/app/src/host.ts).
//
// Screens call host.request(name, args) and nothing else: never a native
// module, never the network. Only this file knows who answers:
//
//   child-module   the child form's Kotlin module (modules/limo-child), linked
//                  in the child form only
//   none           nothing native to ask yet: the guardian form today, whose
//                  link to the master PC is a later piece
//   mock           a development run with no native module (never in a
//                  release build)
//
// A later place joins here, not in the screens: the guardian's link on the
// phone, and the console in a Tauri window on the PC (proof six found it by
// __TAURI_INTERNALS__ on the page's global object).
//
// Each request is listed in Requests, with what it is sent and what it
// answers, so a screen cannot ask for something that does not exist.

import { Platform } from 'react-native';

import { limoChild } from '../modules/limo-child';
import { appForm, type Form } from './form';

export type Place = 'child-module' | 'none' | 'mock';

/** What the start screens show: which form, and who answered. */
export type About = {
  form: Form;
  answeredBy: Place;
  /** The child module's version, or null when it is not linked. */
  childModule: string | null;
  /** Android's version, such as "13", or the platform's name elsewhere. */
  system: string;
};

export type Requests = {
  about: { args: Record<string, never>; answer: About };
};

export type RequestName = keyof Requests;

/** A request that could not be answered. `code` says why. */
export class HostError extends Error {
  constructor(
    readonly code: 'UnknownRequest' | 'NotBuiltYet' | 'Failed',
    message: string,
  ) {
    super(message);
    this.name = 'HostError';
  }
}

function place(): Place {
  if (limoChild) return 'child-module';
  return __DEV__ ? 'mock' : 'none';
}

function systemName(): string {
  if (Platform.OS !== 'android') return Platform.OS;
  const release = (Platform.constants as { Release?: string }).Release;
  return release ?? `API ${Platform.Version}`;
}

// What JavaScript can answer by itself, where no native module is linked.
function answerHere(name: RequestName, where: Place): Requests[RequestName]['answer'] {
  switch (name) {
    case 'about':
      return { form: appForm(), answeredBy: where, childModule: null, system: systemName() };
  }
}

async function askChildModule(name: RequestName, args: object): Promise<Requests[RequestName]['answer']> {
  if (!limoChild) throw new HostError('NotBuiltYet', 'The child module is not linked in this build.');
  let answer: unknown;
  try {
    answer = await limoChild.request(name, args as Record<string, unknown>);
  } catch (e) {
    const code = (e as { code?: unknown }).code === 'UnknownRequest' ? 'UnknownRequest' : 'Failed';
    throw new HostError(code, e instanceof Error ? e.message : String(e));
  }
  switch (name) {
    case 'about': {
      const a = answer as { form?: unknown; module?: unknown; android?: unknown };
      return {
        form: a.form === 'child' ? 'child' : appForm(),
        answeredBy: 'child-module',
        childModule: typeof a.module === 'string' ? a.module : null,
        system: typeof a.android === 'string' ? a.android : systemName(),
      };
    }
  }
}

async function request<N extends RequestName>(name: N, args: Requests[N]['args']): Promise<Requests[N]['answer']> {
  const where = place();
  const answer = where === 'child-module' ? await askChildModule(name, args) : answerHere(name, where);
  return answer as Requests[N]['answer'];
}

export const host = { form: appForm, place, request };
