//! Shaun White Skateboarding Animations and AnimSets: bodies laid out here by hand and their refusals, and every
//! Animation and AnimSet of every data forge of `installs.shaunwhite` read to its exact end, the clips' key times
//! ending on their length, the sets' pairs naming clips and the tracks' first keys measured against the bind pose of
//! the bones they name.
mod inputs;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;

use anvil_formats::data::read_entry;
use anvil_formats::forge::Forge;
use anvil_formats::objects::read_objects;
use anvil_formats::shaunwhite::animation::{
    ANIM_PAIR, ANIM_SET, ANIM_TRACK_DATA, ANIMATION, AnimSet, Animation, Error, MAPPING, Values,
};
use anvil_formats::shaunwhite::skeleton::{SKELETON, Skeleton};
use inputs::{Game, install};

fn words(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// A track block: `tag`, its in-memory size, the key count, the times after the first and the values.
fn block(tag: u8, size: u32, times: &[u8], values: &[u8], keys: u32) -> Vec<u8> {
    let mut b = vec![tag];
    b.extend(words(&[size, keys]));
    b.extend(times);
    b.extend(values);
    b
}

/// A clip of length 1/3 s (20 ticks) with one event's bytes `events` (`count` events), a root translation in packed
/// millimetres and a bone rotation in 48 bits.
fn clip(count: u32, events: &[u8], blocks: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![2, 0, 0];
    b.extend(words(&[1, 0]));
    b.extend([0, 0, 7, 0]);
    b.extend(words(&[99]));
    b.push(1);
    b.extend(words(&[40, ANIMATION]));
    b.extend((1f32 / 3.0).to_le_bytes());
    b.extend(words(&[0xABCD, 0]));
    b.extend([0, 0]);
    b.extend(words(&[43, count]));
    b.extend(events);
    b.extend(words(&[0]));
    b.push(0);
    b.extend(words(&[0, ANIM_TRACK_DATA, 0xABCD, 2]));
    b.extend(words(&[0, MAPPING, 0, 0, MAPPING, 0x1234]));
    b.extend([0, 0]);
    b.extend(words(&[blocks.len() as u32]));
    for k in blocks {
        b.extend(k);
    }
    b
}

/// The root's translation: two keys (at 0 and 20 ticks) of kind 8, x = -1 mm, y = 2 mm, z = 511 mm, then x = 1023
/// mm, y = -1024 mm, z = -512 mm.
fn translation() -> Vec<u8> {
    let pack =
        |x: i32, y: i32, z: i32| ((x as u32 & 0x7FF) << 21) | ((y as u32 & 0x7FF) << 10) | (z as u32 & 0x3FF);
    block(
        0x20,
        12,
        &[20],
        &words(&[pack(-1, 2, 511), pack(1023, -1024, -512)]),
        2,
    )
}

/// A bone's rotation: one key of kind 4, the identity (w left out, x, y and z at the middle of their range).
fn rotation() -> Vec<u8> {
    let mid = 0x3FFFu16 + 1;
    let w = [mid | 0x8000, mid | 0x8000, mid];
    block(
        0x10,
        8,
        &[],
        &w.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>(),
        1,
    )
}

#[test]
fn hand_built_animation_reads_and_refuses_what_does_not_fit() {
    let a = Animation::parse(&clip(0, &[], &[translation(), rotation()])).unwrap();
    assert_eq!(
        (a.id, a.layout, a.track_data_layout, a.unknown_id),
        (40, 0xABCD, 0xABCD, 43)
    );
    assert_eq!(a.table.len(), 1);
    assert_eq!((a.table[0].kind, a.table[0].count), (7, 99));
    assert_eq!(a.mappings, [0, 0x1234]);
    let t = &a.tracks[0];
    assert_eq!(
        (t.kind(), t.times.clone(), t.is_translation()),
        (8, vec![0, 20], true)
    );
    assert_eq!(t.seconds(), [0.0, 1.0 / 3.0]);
    let Values::Translations(v) = t.values().unwrap() else {
        panic!("kind 8 is a translation")
    };
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
    assert!(
        close(v[0], [-0.001, 0.002, 0.511]) && close(v[1], [1.023, -1.024, -0.512]),
        "{v:?}"
    );
    let Values::Rotations(q) = a.tracks[1].values().unwrap() else {
        panic!("kind 4 is a rotation")
    };
    assert!(
        q[0].iter()
            .zip([0.0, 0.0, 0.0, 1.0])
            .all(|(x, y)| (x - y).abs() < 1e-4),
        "{q:?}"
    );

    // events are kept as bytes: the track data is found after them
    let events = [0u8, 0, 0, 0, 0, 0x13, 0x22, 0x68, 0x41, 7];
    let a = Animation::parse(&clip(1, &events, &[translation(), rotation()])).unwrap();
    assert_eq!((a.events.count, a.events.bytes.as_slice()), (1, &events[..]));

    // a wrong size field, an unknown kind, a missing track and a trailing byte are refused
    let mut wrong = translation();
    wrong[1] = 16;
    assert!(matches!(
        Animation::parse(&clip(0, &[], &[wrong, rotation()])),
        Err(Error::Track { track: 0, .. })
    ));
    let mut unknown = rotation();
    unknown[0] = 0x30;
    assert!(matches!(
        Animation::parse(&clip(0, &[], &[translation(), unknown])),
        Err(Error::Kind { track: 1, tag: 0x30 })
    ));
    let mut short = clip(0, &[], &[translation(), rotation()]);
    short.pop();
    assert!(matches!(Animation::parse(&short), Err(Error::Short(_))));
    let mut long = clip(0, &[], &[translation(), rotation()]);
    long.push(0);
    assert!(matches!(Animation::parse(&long), Err(Error::Trailing(1))));
    assert!(matches!(
        Animation::parse(&clip(0, &[], &[translation()])),
        Err(Error::Tracks {
            tracks: 1,
            mappings: 2
        })
    ));
    let mut table = clip(0, &[], &[translation(), rotation()]);
    table[0] = 3;
    assert!(matches!(Animation::parse(&table), Err(Error::Value { .. })));
}

#[test]
fn hand_built_anim_set_reads_and_refuses_what_does_not_fit() {
    let mut b = vec![1];
    b.extend(words(&[
        5, ANIM_SET, 0, 2, 6, ANIM_PAIR, 100, 101, 7, ANIM_PAIR, 101, 100,
    ]));
    b.push(1);
    let s = AnimSet::parse(&b).unwrap();
    assert_eq!((s.id, s.unknown, s.flag, s.pairs.len()), (5, 0, 1, 2));
    assert_eq!(
        (s.pairs[1].id, s.pairs[1].first, s.pairs[1].second),
        (7, 101, 100)
    );
    let mut long = b.clone();
    long.push(0);
    assert!(matches!(AnimSet::parse(&long), Err(Error::Trailing(1))));
    let mut class = b.clone();
    class[22] ^= 1;
    assert!(matches!(AnimSet::parse(&class), Err(Error::Value { .. })));
    assert!(matches!(AnimSet::parse(&b[..b.len() - 1]), Err(Error::Short(_))));
}

type Quat = [f32; 4];
/// A bone's bind rotation and position.
type Bind = (Quat, [f32; 4]);

/// 1 - |dot|: 0 where two unit quaternions are the same rotation.
fn apart(a: Quat, b: Quat) -> f32 {
    1.0 - a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>().abs()
}

#[test]
fn every_real_animation_and_anim_set_reads() {
    let Some(folder) = install(Game::ShaunWhite) else {
        return;
    };
    let mut forges: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "forge"))
        .filter(|p| {
            !p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("DataPC_StreamedSounds")
        })
        .collect();
    forges.sort();
    let mut n: BTreeMap<&str, usize> = BTreeMap::new();
    let mut clips: BTreeSet<u32> = BTreeSet::new();
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    let mut layouts: BTreeMap<u32, BTreeSet<Vec<u32>>> = BTreeMap::new();
    // every bind rotation and position of a bone of that name, in any Skeleton
    let mut binds: BTreeMap<u32, Vec<Bind>> = BTreeMap::new();
    let mut first_keys: Vec<(u32, bool, Values)> = Vec::new();
    for path in &forges {
        let f = Forge::open(path).unwrap();
        let mut r = File::open(path).unwrap();
        for (k, name) in f.names.iter().enumerate() {
            if name.name == "GlobalMetaFile" {
                continue;
            }
            let e = read_entry(&f.payload(&mut r, k).unwrap(), 1 << 30).unwrap();
            for o in read_objects(&e.metadata.data, &e.resource.data).unwrap() {
                let at = || format!("{}: {}: {}", path.display(), name.name, o.name);
                match o.class {
                    ANIMATION => {
                        assert_eq!(o.header, 1, "{}", at());
                        let a = Animation::parse(&e.resource.data[o.body.start..o.trailer.end])
                            .unwrap_or_else(|err| panic!("{}: {err}", at()));
                        assert_eq!(a.id, o.id, "{}", at());
                        // the declared body is the object without its header byte and table
                        assert_eq!(o.trailer.len(), 7 + 12 * a.table.len(), "{}", at());
                        *n.entry("clips").or_default() += 1;
                        if clips.insert(a.id) {
                            *n.entry("distinct clips").or_default() += 1;
                        }
                        *n.entry("tracks").or_default() += a.tracks.len();
                        *n.entry("clips with events").or_default() += usize::from(a.events.count > 0);
                        *n.entry("layout words equal").or_default() +=
                            usize::from(a.layout == a.track_data_layout);
                        let end = (a.length * 60.0).round() as u16;
                        for (bone, t) in a.bone_tracks() {
                            assert!(t.times.windows(2).all(|w| w[0] < w[1]), "{}", at());
                            *n.entry("tracks ending on the length").or_default() +=
                                usize::from(t.times.last() == Some(&end));
                            let v = t.values().unwrap_or_else(|err| panic!("{}: {err}", at()));
                            if let Values::Rotations(q) = &v {
                                assert!(
                                    q.iter()
                                        .all(|q| (q.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-3)
                                );
                            }
                            if bone != 0 && (t.is_rotation() || t.is_translation()) {
                                first_keys.push((bone, t.is_rotation(), v));
                            }
                        }
                        layouts.entry(a.layout).or_default().insert(a.mappings.clone());
                    }
                    ANIM_SET => {
                        let s = AnimSet::parse(&e.resource.data[o.body.clone()])
                            .unwrap_or_else(|err| panic!("{}: {err}", at()));
                        assert_eq!(s.id, o.id, "{}", at());
                        *n.entry("sets").or_default() += 1;
                        pairs.extend(s.pairs.iter().map(|p| (p.first, p.second)));
                    }
                    SKELETON => {
                        let s = Skeleton::parse(&e.resource.data[o.body.clone()]).unwrap();
                        for b in &s.bones {
                            binds
                                .entry(b.name)
                                .or_default()
                                .push((b.local_rotation, b.local_position));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    n.insert("pairs", pairs.len());
    n.insert(
        "pairs naming two clips",
        pairs
            .iter()
            .filter(|(a, b)| clips.contains(a) && clips.contains(b))
            .count(),
    );
    n.insert("layout words", layouts.len());
    n.insert(
        "layout words with one mapping list",
        layouts.values().filter(|m| m.len() == 1).count(),
    );
    for (bone, rotation, v) in &first_keys {
        let Some(b) = binds.get(bone) else {
            *n.entry("first keys of a bone no Skeleton has").or_default() += 1;
            continue;
        };
        match v {
            Values::Rotations(q) if *rotation => {
                *n.entry("rotation first keys").or_default() += 1;
                let best = b.iter().map(|(r, _)| apart(q[0], *r)).fold(f32::MAX, f32::min);
                *n.entry("rotation first keys at a bind rotation (1e-5)")
                    .or_default() += usize::from(best < 1e-5);
            }
            Values::Translations(p) => {
                *n.entry("translation first keys").or_default() += 1;
                let best = b
                    .iter()
                    .map(|(_, t)| (0..3).map(|i| (p[0][i] - t[i]).abs()).fold(0.0, f32::max))
                    .fold(f32::MAX, f32::min);
                *n.entry("translation first keys within 1 mm of a bind position")
                    .or_default() += usize::from(best <= 0.0011);
            }
            _ => {}
        }
    }
    println!("{n:#?}");
    assert!(n["clips"] > 0 && n["sets"] > 0);
    assert_eq!(n["pairs naming two clips"], n["pairs"]);
    assert_eq!(n["tracks ending on the length"] + 25, n["tracks"]);
}
