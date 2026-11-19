// The preview of the child form's screens, in a browser on the PC, so they can
// be checked against the drawings without a phone. DEVELOPMENT ONLY: it is
// App.web.tsx's page; Android's bundle takes App.tsx and never reaches it.
//
// Every screen in every state, at 390 by 844 and at 360 by 640, each a live
// child form driven by its own mock host: taps work, and opening a step's
// Android screen switches the step on after a moment, as a guardian would.
// Then long names and large text.
//
// With "#one=<id>&size=390" (or 360) in the address it draws one phone and
// nothing else, for pictures; "&scale=1.3" sets the text scale.

import { type ReactNode, useState } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';

import { ChildApp } from '../child/ChildApp';
import { ChildHostProvider } from '../child/HostContext';
import { mockHost } from '../mock/childMock';
import { color, tokens } from '../theme';
import FONTS from '../theme/fonts.json';
import { Txt, type } from '../ui/paper/text';
import { findFrame, type Frame, FRAMES, STRESS } from './frames';
import { PhoneFrame, type Size, SIZES } from './PhoneFrame';

// The theme's fonts, from copies scripts/preview.mjs puts beside the page.
// A browser sets Fraunces' optical size to the font size by itself.
const FONT_FILES = { display: 'fonts/fraunces.ttf', body: 'fonts/instrument-sans.ttf', mono: 'fonts/spline-sans-mono.ttf' };
function addFonts() {
  if (typeof document === 'undefined' || document.getElementById('limo-fonts')) return;
  const face = (family: string, file: string, weights: string) =>
    `@font-face { font-family: '${family}'; src: url('${file}') format('truetype'); font-weight: ${weights}; font-display: block; }`;
  const css = [
    face(tokens.font.body, FONT_FILES.body, '400 700'),
    face(tokens.font.mono, FONT_FILES.mono, '300 700'),
    ...FONTS.displaySizes.map((size) => face(`${tokens.font.display} ${size}`, FONT_FILES.display, '100 900')),
    'body { background: #e9e3d3; }',
  ].join('\n');
  const style = document.createElement('style');
  style.id = 'limo-fonts';
  style.textContent = css;
  document.head.appendChild(style);
}
addFonts();

function Live({ frame }: { frame: Frame }) {
  const [host] = useState(() => mockHost(frame.state, { delayMs: 700 }));
  return (
    <ChildHostProvider host={host}>
      <ChildApp initial={frame.route} />
    </ChildHostProvider>
  );
}

function Shot({ frame, size, textScale }: { frame: Frame; size: Size; textScale?: number }) {
  return (
    <View style={styles.shot}>
      <View style={styles.cap}>
        <Txt style={[type.label, styles.capTitle]}>{frame.title}</Txt>
        <Txt style={type.label}>
          {`${size.width} by ${size.height}${textScale && textScale !== 1 ? ` · text ${textScale}` : ''}${frame.note ? ` · ${frame.note}` : ''}`}
        </Txt>
      </View>
      <PhoneFrame size={size} textScale={textScale}>
        <Live frame={frame} />
      </PhoneFrame>
    </View>
  );
}

function Section({ title, lead, children }: { title: string; lead: string; children: ReactNode }) {
  return (
    <View style={styles.section}>
      <Txt style={type.h1}>{title}</Txt>
      <Txt style={[type.detail, styles.lead]}>{lead}</Txt>
      <View style={styles.shots}>{children}</View>
    </View>
  );
}

function readHash(): { one?: string; size: Size; scale: number } {
  const hash = typeof window === 'undefined' ? '' : window.location.hash.slice(1);
  const q = new URLSearchParams(hash);
  const size = q.get('size') === '360' ? SIZES[360] : SIZES[390];
  const scale = Number(q.get('scale') ?? '1');
  return { one: q.get('one') ?? undefined, size, scale: Number.isFinite(scale) && scale > 0 ? scale : 1 };
}

export function Gallery() {
  const [{ one, size, scale }] = useState(readHash);
  const single = one ? findFrame(one) : undefined;
  if (single) {
    return (
      <PhoneFrame size={size} textScale={scale}>
        <Live frame={single} />
      </PhoneFrame>
    );
  }
  return (
    <ScrollView style={styles.page} contentContainerStyle={styles.content}>
      <Txt style={type.label}>LiMo · the child form · preview · development only</Txt>
      <Txt style={[type.h1, styles.title]}>The child’s phone, from the app’s own code</Txt>
      <Txt style={[type.detail, styles.lead]}>
        Each phone is the real child form in React Native, drawn by react-native-web and driven by a mock host that plays one
        state. Taps work. Open design/screens/phone-setup.html and phone.html beside it to compare.
      </Txt>
      <Section title="390 by 844" lead="The size the drawings are laid out for.">
        {FRAMES.map((f) => (
          <Shot key={f.id} frame={f} size={SIZES[390]} />
        ))}
      </Section>
      <Section title="360 by 640" lead="The small phone: the page scrolls above the pinned button and under the tab bar.">
        {FRAMES.map((f) => (
          <Shot key={f.id} frame={f} size={SIZES[360]} />
        ))}
      </Section>
      <Section title="Long names, large text" lead="A long child’s name and long app names, with text at 1.3 and at 2 times its size.">
        {STRESS.map((f) => (
          <Shot key={`${f.id} 1.3`} frame={f} size={SIZES[360]} textScale={1.3} />
        ))}
        {STRESS.map((f) => (
          <Shot key={`${f.id} 2`} frame={f} size={SIZES[390]} textScale={2} />
        ))}
      </Section>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: color.paper },
  content: { padding: 28, paddingBottom: 60 },
  title: { marginTop: 8 },
  lead: { marginTop: 6, maxWidth: 760 },
  section: { marginTop: 40 },
  shots: { marginTop: 18, flexDirection: 'row', flexWrap: 'wrap', alignItems: 'flex-start', columnGap: 32, rowGap: 36 },
  shot: { flexShrink: 0 },
  cap: { minHeight: 44, marginBottom: 10, gap: 6, maxWidth: 393 },
  capTitle: { color: color.ink },
});
