import { ChildApp } from '../../src/child/ChildApp';

/**
 * The child form: setup, the child's introduction, then Today and How it
 * works. It asks the real host (src/host.ts) and nothing else; where the
 * child module does not answer yet, the phone is not paired and it shows
 * "This phone is not set up yet."
 */
export default function ChildStart() {
  return <ChildApp />;
}
