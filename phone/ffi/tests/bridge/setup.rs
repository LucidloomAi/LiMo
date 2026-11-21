//! Setup through this layer: a phone just paired counts and covers nothing,
//! its time left and its view say setup is not finished; finished once, it
//! counts and covers as any phone, stays finished in the kept bytes, and
//! finishing again changes nothing.

use limo_phone_ffi::{CoverKind, EngineError, PhoneEngine};

use crate::common::*;

const NOT_FINISHED: &str = "Setup is not finished on this phone.";

#[test]
fn nothing_counts_or_is_covered_until_setup_is_finished_and_then_it_stays_finished() {
    let mut phone = Phone::setting_up(at(16, 0));
    let view = phone.engine.view().unwrap().unwrap();
    assert!(!view.setup_finished);
    assert!(view.json.contains(&format!(r#""setup":{{"finished":false,"finished_at":null,"words":"{NOT_FINISHED}"}}"#)), "{}", view.json);

    // A game and the blocked casino, during setup: nothing counted, nothing covered.
    let orders = phone.run(60);
    assert_eq!(orders.cover.kind, CoverKind::Nothing);
    assert!(!orders.time_left.counting);
    assert_eq!(orders.time_left.words, NOT_FINISHED);
    phone.front = app(CASINO, "Casino");
    assert_eq!(phone.run(5).cover.kind, CoverKind::Nothing);
    assert_eq!(phone.counted(), 0);
    // Nothing noted of the casino: only how the phone stands, for the
    // guardian watching the steps on the PC (six on, and no protection yet).
    let standing = |n: &String| n.contains(r#""note":"permission_on""#) || n.contains(r#""note":"protection","level":"off""#);
    assert_eq!(phone.notes.len(), 7, "{:?}", phone.notes);
    assert!(phone.notes.iter().all(standing), "{:?}", phone.notes);

    // Finished: kept at once, noted once, covering and counting from the next call.
    assert!(phone.engine.finish_setup(phone.wall).unwrap());
    assert!(phone.engine.should_save().unwrap());
    phone.kept = phone.engine.save().unwrap();
    let view = phone.engine.view().unwrap().unwrap();
    assert!(view.setup_finished);
    assert!(view.json.contains(&format!(r#""setup":{{"finished":true,"finished_at":{},"words":null}}"#, phone.wall)), "{}", view.json);
    assert!(view.json.contains(r#""note":"setup_finished""#), "{}", view.json);
    assert_eq!(phone.run(1).cover.kind, CoverKind::Block);
    phone.front = app(GAME, "Game");
    phone.tick();
    phone.run(30);
    assert_eq!(phone.counted(), 30);

    // Again: nothing changes, nothing to keep.
    phone.kept = phone.engine.save().unwrap();
    assert!(!phone.engine.finish_setup(phone.wall).unwrap());
    assert!(!phone.engine.should_save().unwrap());

    // It stays finished in the kept bytes.
    phone.crash();
    assert!(phone.engine.view().unwrap().unwrap().setup_finished);

    // A state that cannot be read cannot be finished.
    let unreadable = PhoneEngine::load(Vec::new());
    assert!(matches!(unreadable.finish_setup(phone.wall), Err(EngineError::CannotCheck { .. })));
}
