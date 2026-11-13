//! Images name a prelude unit's code by the unit's chunk and record
//! offset (`CODE_PRELUDE`, src/snapshot.rs), and the build id covers the
//! packed chunks (the prelude hash), not the unit table. So a prelude edit
//! must either move the hash or leave every unit's record where it was;
//! otherwise an image would restore other code under the same build id.
#![allow(clippy::expect_used, reason = "tests")]

#[path = "../build.rs"]
#[allow(dead_code, reason = "the build script's other parts")]
mod build;

use build::UnitPlaces as Places;

fn places(src: &str) -> Places {
    build::unit_places(src).expect("the prelude builds")
}

/// Every unit's reference in `before` is valid in `after`: the same hash,
/// and each unit of `before` with a record at the same place.
fn refs_stay_valid(before: &Places, after: &Places) -> bool {
    before.0 == after.0
        && before
            .1
            .iter()
            .all(|(n, p)| p.is_none() || after.1.get(n) == Some(p))
}

#[test]
fn a_prelude_edit_moves_the_hash_or_keeps_every_unit_in_place() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/prelude.py"))
        .expect("prelude.py");
    let base = places(&src);
    let target = base
        .1
        .iter()
        .find_map(|(n, p)| p.map(|_| n.clone()))
        .expect("a unit with a record");
    // An alias sorts first and has no record: every unit's index in the
    // table moves (the experiment that found this), but no record does.
    let alias = places(&format!("{src}\naaaa_alias = {target}\n"));
    assert!(
        base.0 != alias.0 || refs_stay_valid(&base, &alias),
        "an alias line moved a unit's record and left the prelude hash"
    );
    // A function before the others moves records, and the hash with them.
    let func = places(&format!("def aaaa_first(x):\n    return x + 1\n\n{src}"));
    assert!(
        base.0 != func.0 || refs_stay_valid(&base, &func),
        "a new function moved a unit's record and left the prelude hash"
    );
}
