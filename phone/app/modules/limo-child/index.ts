import { requireOptionalNativeModule } from 'expo';

/** The child form's native module, as JavaScript sees it. */
export type LimoChildNative = {
  /**
   * Sends one request to the child module. Rejects with an Error whose `code`
   * is "UnknownRequest" when the module has no request of that name.
   */
  request(name: string, args: Record<string, unknown>): Promise<unknown>;
};

/**
 * The module, or null where it is not linked: in the guardian form, which
 * leaves the module out, and anywhere that is not an Android build.
 *
 * Only src/host.ts uses this. Screens go through the host.
 */
export const limoChild = requireOptionalNativeModule<LimoChildNative>('LimoChild');
