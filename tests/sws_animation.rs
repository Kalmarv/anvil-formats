//! Shaun White Skateboarding Animations and AnimSets: bodies laid out here by hand and their refusals, and every
//! Animation and AnimSet of every data forge of `installs.shaunwhite` read to its exact end, the clips' key times
//! ending on their length, the sets' pairs naming clips and the tracks' first keys measured against the bind pose of
//! the bones they name.
mod inputs;
mod skinning;

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

/// The middle of a 15-bit component's range: 0.
const MID: u16 = 0x4000;

/// A bone's rotation: one key of kind 4, the identity (w left out, x, y and z at the middle of their range).
fn rotation() -> Vec<u8> {
    rotation_key([MID | 0x8000, MID | 0x8000, MID])
}

/// A rotation track of one kind 4 key, its three words `w`.
fn rotation_key(w: [u16; 3]) -> Vec<u8> {
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

    // kind 4's index: bit 15 of the first word its low bit, of the second its high bit
    for (w, left_out) in [
        ([MID | 0x8000, MID, MID], [0.0, 1.0, 0.0, 0.0]),
        ([MID, MID | 0x8000, MID], [0.0, 0.0, 1.0, 0.0]),
        ([MID, MID, MID], [1.0, 0.0, 0.0, 0.0]),
    ] {
        let a = Animation::parse(&clip(0, &[], &[translation(), rotation_key(w)])).unwrap();
        let Values::Rotations(q) = a.tracks[1].values().unwrap() else {
            panic!("kind 4 is a rotation")
        };
        assert!(
            q[0].iter().zip(left_out).all(|(x, y)| (x - y).abs() < 1e-4),
            "{w:x?}: {q:?}"
        );
    }

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
    let mut first_keys: Vec<(u32, u8, Values)> = Vec::new();
    let mut changes: BTreeMap<[u16; 2], (usize, usize)> = BTreeMap::new();
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
                        let distinct = clips.insert(a.id);
                        if distinct {
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
                                if t.kind() == 4 && distinct {
                                    index_changes(&t.values, q, &mut changes);
                                }
                            }
                            if bone != 0 && (t.is_rotation() || t.is_translation()) {
                                first_keys.push((bone, t.kind(), v));
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
    // by kind: (first keys at a bind rotation within 1e-5 or a bind position within 1 mm, first keys of a bone a
    // Skeleton has)
    let mut at_bind: BTreeMap<u8, (usize, usize)> = BTreeMap::new();
    for (bone, kind, v) in &first_keys {
        let Some(b) = binds.get(bone) else {
            *n.entry("first keys of a bone no Skeleton has").or_default() += 1;
            continue;
        };
        let at = match v {
            Values::Rotations(q) => b.iter().map(|(r, _)| apart(q[0], *r)).fold(f32::MAX, f32::min) < 1e-5,
            Values::Translations(p) => {
                b.iter()
                    .map(|(_, t)| (0..3).map(|i| (p[0][i] - t[i]).abs()).fold(0.0, f32::max))
                    .fold(f32::MAX, f32::min)
                    <= 0.0011
            }
            _ => continue,
        };
        let e = at_bind.entry(*kind).or_default();
        e.0 += usize::from(at);
        e.1 += 1;
    }
    println!("{n:#?}\nfirst keys at the bind pose by kind (at it, of): {at_bind:?}");
    assert!(n["clips"] > 0 && n["sets"] > 0);
    assert_eq!(n["pairs naming two clips"], n["pairs"]);
    assert_eq!(n["tracks ending on the length"] + 25, n["tracks"]);
    // the first keys at the bind pose, by kind (all 80,466 objects, a clip stored twice counted twice): pinned, so a
    // change to a decode shows. Kind 4's index bits read the other way round give 198,380 at a bind rotation.
    assert_eq!(
        at_bind,
        BTreeMap::from([
            (4, (205_131, 3_865_438)),
            (5, (10_098, 238_030)),
            (6, (59_960, 942_702)),
            (8, (95_872, 519_002)),
            (9, (1_009, 95_187))
        ])
    );
    // kind 4's index bits: a key whose omitted component differs from the key before it decodes to a rotation
    // beside that key's (96% to 99% of each kind of change with |dot| > 0.9), as consecutive keys of one index do
    // (99.6% to 100%). Read with the two bits the other way round, as Riders Republic's are, the eight changes between
    // index 0 or 3 and index 1 or 2 keep 1% to 24%.
    println!(
        "kind 4 consecutive keys whose index bits change, by the bits before and after (bit 15 of the first word, as 2, plus that of the second): (pairs, |dot| > 0.9) {changes:?}"
    );
    assert_eq!(changes.len(), 12);
    for (bits, (pairs, smooth)) in &changes {
        assert!(*smooth * 10 >= *pairs * 9, "{bits:?}: {smooth} of {pairs}");
    }
}

/// Counts, by the stored bits that change, the consecutive keys of a kind 4 track (`values` as stored, `q` decoded)
/// whose index bits differ, and those of them whose rotations are within |dot| > 0.9 of each other.
fn index_changes(values: &[u8], q: &[[f32; 4]], changes: &mut BTreeMap<[u16; 2], (usize, usize)>) {
    let bits: Vec<u16> = values
        .chunks(6)
        .map(|w| (u16::from(w[1] >> 7) << 1) | u16::from(w[3] >> 7))
        .collect();
    for (b, q) in bits.windows(2).zip(q.windows(2)) {
        if b[0] != b[1] {
            let e = changes.entry([b[0], b[1]]).or_default();
            e.0 += 1;
            e.1 += usize::from(q[0].iter().zip(q[1]).map(|(x, y)| x * y).sum::<f32>().abs() > 0.9);
        }
    }
}

/// The tracks' value at `tick`: linear between the keys around it (rotations normalized, the second key's sign
/// taken to the first's side), the first or last key outside them.
fn sample<const N: usize>(times: &[u16], values: &[[f32; N]], tick: f64) -> [f64; N] {
    let at = times.partition_point(|&t| f64::from(t) <= tick);
    let widen = |v: &[f32; N]| v.map(f64::from);
    if at == 0 {
        return widen(&values[0]);
    }
    if at == times.len() {
        return widen(&values[at - 1]);
    }
    let (a, b) = (widen(&values[at - 1]), widen(&values[at]));
    let u = (tick - f64::from(times[at - 1])) / f64::from(times[at] - times[at - 1]);
    let flip = if N == 4 && a.iter().zip(&b).map(|(x, y)| x * y).sum::<f64>() < 0.0 {
        -1.0
    } else {
        1.0
    };
    let mut v: [f64; N] = std::array::from_fn(|i| a[i] + (flip * b[i] - a[i]) * u);
    if N == 4 {
        let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        v = v.map(|x| x / n);
    }
    v
}

/// A clip's decoded tracks by bone: rotations and translations with their key times.
type Decoded = (
    BTreeMap<u32, (Vec<u16>, Vec<[f32; 4]>, u8)>,
    BTreeMap<u32, (Vec<u16>, Vec<[f32; 3]>, u8)>,
);

fn decoded(a: &Animation) -> Decoded {
    let (mut rot, mut tr) = (BTreeMap::new(), BTreeMap::new());
    for (bone, t) in a.bone_tracks() {
        match t.values().expect("the census reads every track's values") {
            Values::Rotations(q) => {
                rot.insert(bone, (t.times.clone(), q, t.kind()));
            }
            Values::Translations(p) => {
                tr.insert(bone, (t.times.clone(), p, t.kind()));
            }
            _ => {}
        }
    }
    (rot, tr)
}

/// The renderer's bone matrices are the decoded clips: for a traced skinned frame (the `runtime` oracle), one stored
/// clip, sampled at the frame's time (to a hundredth of a tick), gives every traced local rotation of the frame's
/// child bones (1 − |dot| ≤ 1e-6) and every traced local translation of the bones it has a translation track for
/// (≤ 0.1 mm). It does so for exactly 24 of the trace's 156 frames: the train's in frames 7201 to 7204 and all twenty
/// of the walking worker's two meshes (legs and torso), with rotation kinds 5 and 6 and translation kinds 8 and 9.
/// The other 132 are not one clip's pose, and the test fails if one of them becomes one:
/// - the train in its two frames before (7199, 7200), at its bind pose, which the clip gives to 2 mm only, and in
///   its ten later frames (8000 to 8180), where the same clip, at times rising 13.3 ticks a step, gives every bone
///   the clip moves but one, which something else drives;
/// - the five pigeons (60 frames): no clip has a rotation track for each of their twelve bones; the one that drives
///   eleven of them gives seven at a steady rate, not the rest;
/// - two walking characters' four meshes and two heads (59 frames): no stored clip gives any of their bones, of any
///   kind, at any time, with a bone a clip has no track for at its bind rotation, and those meshes' child bones lie up
///   to 2.4 m from their stored positions with no clip translating them (`tests/sws_skeleton.rs`). These are the 26
///   frames whose nearest clip is one of kind 4, so kind 4 is not checked here.
#[test]
fn traced_skinned_frames_are_decoded_clip_poses() {
    let (Some(folder), Some(table)) = (
        install(Game::ShaunWhite),
        inputs::oracle(inputs::OracleSet::Runtime, Game::ShaunWhite, "skin.tsv"),
    ) else {
        return;
    };
    let draws = skinning::draws(&table);
    let data = skinning::install(&folder, &draws, true);
    // each traced (mesh, frame): its child bones' local transforms, over all its draw ranges
    let mut frames: BTreeMap<(String, usize, u32, u32), BTreeMap<u32, skinning::M4>> = BTreeMap::new();
    for d in &draws {
        let (mesh, palettes) = &data.meshes[&(d.forge.clone(), d.entry, d.mesh)];
        for l in skinning::locals(d, mesh, palettes, &data.skeletons).unwrap() {
            frames
                .entry((d.forge.clone(), d.entry, d.mesh, d.frame))
                .or_default()
                .insert(l.bone, l.traced);
        }
    }
    let mut cache: BTreeMap<u32, Decoded> = BTreeMap::new();
    let (mut reproduced, mut unanimated, mut kinds) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for ((forge, _, mesh, frame), bones) in &frames {
        let mut best: Option<(f64, f64, u32, f64)> = None;
        for a in &data.animations {
            if !bones
                .keys()
                .all(|b| a.bone_tracks().any(|(m, t)| m == *b && t.is_rotation()))
            {
                continue;
            }
            let (rot, tr) = cache.entry(a.id).or_insert_with(|| decoded(a));
            let last = rot
                .values()
                .filter_map(|(t, _, _)| t.last())
                .max()
                .copied()
                .unwrap_or(0);
            let score = |tick: f64| -> f64 {
                bones
                    .iter()
                    .map(|(b, m)| {
                        let (times, q, _) = &rot[b];
                        let t = skinning::rotation(m);
                        1.0 - sample(times, q, tick)
                            .iter()
                            .zip(t)
                            .map(|(x, y)| x * y)
                            .sum::<f64>()
                            .abs()
                    })
                    .fold(0.0, f64::max)
            };
            // whole ticks, then hundredths of a tick around the best: a frame falls between ticks
            let whole = (0..=last)
                .map(f64::from)
                .min_by(|&x, &y| score(x).total_cmp(&score(y)))
                .unwrap_or(0.0);
            let tick = (-100..=100)
                .map(|k| whole + f64::from(k) / 100.0)
                .filter(|&t| t >= 0.0)
                .min_by(|&x, &y| score(x).total_cmp(&score(y)))
                .unwrap_or(whole);
            let r = score(tick);
            if best.is_some_and(|b| b.0 <= r) {
                continue;
            }
            let mut d = 0f64;
            for (b, m) in bones {
                if let Some((times, p, _)) = tr.get(b) {
                    let s = sample(times, p, tick);
                    d = d.max((0..3).map(|i| (s[i] - m[i][3]).abs()).fold(0.0, f64::max));
                }
            }
            best = Some((r, d, a.id, tick));
        }
        let Some((r, d, clip, tick)) = best else {
            unanimated.insert(*mesh);
            println!(
                "{forge}: mesh {mesh} frame {frame}: no stored clip animates its {} bones",
                bones.len()
            );
            continue;
        };
        let ok = r <= 1e-6 && d <= 1e-4;
        let (rot, tr) = &cache[&clip];
        // the kinds of the tracks that drive the frame's bones, and the bones the clip leaves off at that time
        let mut used = BTreeSet::new();
        let mut off = 0usize;
        for (b, m) in bones {
            used.insert(("rotation", rot[b].2));
            let s = sample(&rot[b].0, &rot[b].1, tick);
            off += usize::from(
                1.0 - s
                    .iter()
                    .zip(skinning::rotation(m))
                    .map(|(x, y)| x * y)
                    .sum::<f64>()
                    .abs()
                    > 1e-6,
            );
            if let Some(t) = tr.get(b) {
                used.insert(("translation", t.2));
            }
        }
        println!(
            "{forge}: mesh {mesh} frame {frame}: clip {clip} tick {tick:.2}: rotation 1-|dot| {r:.1e}, translation {d:.1e} m, kinds {used:?}{}",
            if ok {
                String::new()
            } else {
                format!(" (not this clip: {off} of {} bones off)", bones.len())
            }
        );
        if ok {
            reproduced.insert((*mesh, *frame));
            kinds.extend(used);
        }
    }
    println!(
        "{} of {} traced frames reproduced by a decoded clip; track kinds: {kinds:?}",
        reproduced.len(),
        frames.len()
    );
    let (train, legs, torso) = (2_445_827_479u32, 2_881_073_162u32, 2_881_073_252u32);
    let expected: BTreeSet<(u32, u32)> = (7201..=7204)
        .map(|f| (train, f))
        .chain(
            [legs, torso]
                .into_iter()
                .flat_map(|m| (8000..=8180).step_by(20).map(move |f| (m, f))),
        )
        .collect();
    assert_eq!(reproduced, expected);
    let pigeons = BTreeSet::from([
        1_814_156_304,
        1_814_156_345,
        1_814_156_367,
        1_950_109_412,
        1_950_109_447,
    ]);
    assert_eq!(unanimated, pigeons);
    for k in [
        ("rotation", 5),
        ("rotation", 6),
        ("translation", 8),
        ("translation", 9),
    ] {
        assert!(kinds.contains(&k), "{k:?} not among {kinds:?}");
    }
}
