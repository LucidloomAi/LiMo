import { StartScreen } from '../../src/ui/StartScreen';

/** The child form's start screen, until setup is built. */
export default function ChildStart() {
  return (
    <StartScreen
      title="LiMo"
      stamp="CHILD'S PHONE"
      icon="hourglass"
      heading="Setup is not built yet"
      paragraphs={[
        "This is LiMo for a child's phone.",
        'Setup comes in a later build. Until then LiMo counts nothing, covers nothing and limits nothing on this phone.',
      ]}
      checkLine={(about) =>
        about.childModule
          ? `Child module ${about.childModule} answered. Android ${about.system}.`
          : `The child module is missing from this build (${about.answeredBy}). Android ${about.system}.`
      }
    />
  );
}
