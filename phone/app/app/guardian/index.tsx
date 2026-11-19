import { StartScreen } from '../../src/ui/StartScreen';

/** The guardian form's start screen, until setup is built. */
export default function GuardianStart() {
  return (
    <StartScreen
      title="LiMo Guardian"
      stamp="GUARDIAN'S PHONE"
      icon="guardian"
      heading="Setup is not built yet"
      paragraphs={[
        "This is LiMo for a guardian's phone.",
        "Setup comes in a later build. Until then this app is not paired with the family's PC and shows nothing about anyone's time.",
      ]}
      checkLine={(about) =>
        about.childModule
          ? `The child module answered here, which it never should in this form. Android ${about.system}.`
          : `No child module in this form, as it should be. Android ${about.system}.`
      }
    />
  );
}
