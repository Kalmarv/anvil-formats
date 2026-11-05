//! Shaun White Skateboarding TextureMaps: bodies laid out here by hand (a flat map and a cube map) and their refusals,
//! and every TextureMap of every data forge of `installs.shaunwhite` read to its levels.
mod inputs;

use std::collections::BTreeMap;
use std::fs::File;

use anvil_formats::data::read_entry;
use anvil_formats::forge::Forge;
use anvil_formats::objects::read_objects;
use anvil_formats::shaunwhite::texture::{Error, StoredTextureMap, TEXTURE_MAP, d3d_format, storage};
use inputs::{Game, install};

/// A TextureMap body as the game stores it: `fields` are width, height, depth, format code, dimension, gamma and mip
/// count.
fn body(fields: [u32; 7], pixels: &[u8]) -> Vec<u8> {
    let [w, h, d, f, dim, gamma, mips] = fields;
    let mut b = vec![1];
    for v in [
        7,
        TEXTURE_MAP,
        w,
        h,
        d,
        f,
        dim,
        gamma,
        mips,
        1,
        2,
        3,
        4,
        5,
        6,
        9,
        10,
    ] {
        b.extend_from_slice(&u32::to_le_bytes(v));
    }
    b.extend_from_slice(&[4, 0, 0]);
    b.extend_from_slice(&11u32.to_le_bytes());
    b.push(0);
    for v in [8, 0x1323_7FE9, pixels.len() as u32] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(pixels);
    for v in [2, 32, w, h, d, mips, f, dim, gamma, 1, 2, 3, 4, 5, 6] {
        b.extend_from_slice(&u32::to_le_bytes(v));
    }
    b
}

#[test]
fn hand_built_maps_read_and_refuse_what_does_not_fit() {
    // 8 x 4 BC1, three levels: 2 blocks, 1 block, 1 block
    let pixels: Vec<u8> = (0..32).collect();
    let b = body([8, 4, 1, 2, 1, 1, 3], &pixels);
    let t = StoredTextureMap::parse(&b).unwrap();
    assert_eq!(
        (t.id, t.width, t.height, t.format, t.gamma, t.compiled_id),
        (7, 8, 4, 2, 1, 8)
    );
    assert_eq!(
        (t.address, t.filters, t.map_type, t.category),
        ([1, 2, 3], [4, 5, 6], 9, 10)
    );
    assert_eq!((t.unknown_flags, t.user_category), ([4, 0, 0], 11));
    let levels = t.levels().unwrap();
    let sizes: Vec<_> = levels
        .iter()
        .map(|l| (l.mip, l.width, l.height, l.bytes.len()))
        .collect();
    assert_eq!(sizes, [(0, 8, 4, 16), (1, 4, 2, 8), (2, 2, 1, 8)]);
    assert_eq!(&b[levels[1].bytes.clone()], &pixels[16..24]);
    assert_eq!(d3d_format(t.format), Some(u32::from_le_bytes(*b"DXT1")));

    // a 4 x 4 cube map of 32-bit texels and one level: six faces of 64 bytes, face by face
    let cube = body([4, 4, 1, 0, 2, 0, 0], &[0; 6 * 64]);
    let levels = StoredTextureMap::parse(&cube).unwrap().levels().unwrap();
    assert_eq!(levels.len(), 6);
    assert_eq!((levels[5].face, levels[5].mip, levels[5].bytes.len()), (5, 0, 64));

    // pixels one byte short of the levels, a compiled copy that disagrees, a byte after it, another dimension
    let short = body([8, 4, 1, 2, 1, 1, 3], &pixels[..31]);
    assert!(matches!(
        StoredTextureMap::parse(&short).unwrap().levels(),
        Err(Error::Pixels { .. })
    ));
    let mut wrong = b.clone();
    let at = wrong.len() - 4 * 13;
    wrong[at] = 9;
    assert!(matches!(
        StoredTextureMap::parse(&wrong),
        Err(Error::Compiled { what: "width", .. })
    ));
    assert!(matches!(
        StoredTextureMap::parse(&[b.clone(), vec![0]].concat()),
        Err(Error::Trailing(1))
    ));
    assert!(matches!(
        StoredTextureMap::parse(&body([8, 4, 1, 2, 3, 1, 3], &pixels)),
        Err(Error::Value {
            what: "dimension",
            ..
        })
    ));
    assert!(matches!(StoredTextureMap::parse(&b[..60]), Err(Error::Short(_))));
}

/// Every TextureMap object of the 33 data forges reads, and its levels fill its pixels exactly.
#[test]
fn every_real_texture_map_reads_to_its_levels() {
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
    assert_eq!(forges.len(), 33);
    let (mut maps, mut levels) = (0usize, 0usize);
    let mut codes: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for path in &forges {
        let f = Forge::open(path).unwrap();
        let mut r = File::open(path).unwrap();
        for (k, n) in f.names.iter().enumerate() {
            if n.name == "GlobalMetaFile" {
                continue;
            }
            let e = read_entry(&f.payload(&mut r, k).unwrap(), 1 << 30).unwrap();
            for o in read_objects(&e.metadata.data, &e.resource.data).unwrap() {
                if o.class != TEXTURE_MAP {
                    continue;
                }
                let body = &e.resource.data[o.body.clone()];
                let t = StoredTextureMap::parse(body)
                    .unwrap_or_else(|err| panic!("{}: {}: {}: {err}", path.display(), n.name, o.name));
                assert_eq!(t.id, o.id);
                assert!(storage(t.format).is_some() && d3d_format(t.format).is_some());
                levels += t.levels().unwrap().len();
                maps += 1;
                *codes.entry((t.format, t.dimension)).or_default() += 1;
            }
        }
    }
    println!("{maps} TextureMaps, {levels} levels; (format code, dimension): {codes:?}");
    assert_eq!(maps, 40_851);
}

/// The SHA-256 of `bytes`, in lower-case hex.
fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every texture level the game's renderer uploaded (the `runtime` oracle) is the level this reader slices, byte for
/// byte, at the same face and mip and size, and the game created its texture with the format [`d3d_format`] names.
#[test]
fn levels_equal_what_the_game_uploads() {
    let (Some(folder), Some(table)) = (
        install(Game::ShaunWhite),
        inputs::oracle(inputs::OracleSet::Runtime, Game::ShaunWhite, "textures.tsv"),
    ) else {
        return;
    };
    let text = std::fs::read_to_string(table).unwrap();
    // forge -> entry -> rows
    let mut rows: BTreeMap<&str, BTreeMap<usize, Vec<Vec<&str>>>> = BTreeMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        rows.entry(f[0])
            .or_default()
            .entry(f[1].parse().unwrap())
            .or_default()
            .push(f);
    }
    let mut checked = 0usize;
    let mut maps = std::collections::BTreeSet::new();
    for (forge, entries) in &rows {
        let path = folder.join(forge);
        let f = Forge::open(&path).unwrap();
        let mut r = File::open(&path).unwrap();
        for (&k, rows) in entries {
            let e = read_entry(&f.payload(&mut r, k).unwrap(), 1 << 30).unwrap();
            let objs = read_objects(&e.metadata.data, &e.resource.data).unwrap();
            for row in rows {
                let id: u32 = row[2].parse().unwrap();
                let o = objs
                    .iter()
                    .find(|o| o.id == id && o.class == TEXTURE_MAP)
                    .unwrap();
                let body = &e.resource.data[o.body.clone()];
                let t = StoredTextureMap::parse(body).unwrap();
                let (face, mip): (u32, u32) = (row[3].parse().unwrap(), row[4].parse().unwrap());
                let level = t
                    .levels()
                    .unwrap()
                    .into_iter()
                    .find(|l| (l.face, l.mip) == (face, mip))
                    .unwrap();
                assert_eq!(
                    (level.width.to_string(), level.height.to_string()),
                    (row[5].to_string(), row[6].to_string())
                );
                assert_eq!(
                    d3d_format(t.format),
                    Some(u32::from_str_radix(row[7].trim_start_matches("0x"), 16).unwrap())
                );
                assert_eq!(
                    sha256(&body[level.bytes]),
                    row[8],
                    "{forge} {k} {id} face {face} mip {mip}"
                );
                maps.insert((forge.to_string(), k, id));
                checked += 1;
            }
        }
    }
    println!(
        "{checked} uploaded levels of {} TextureMaps equal their stored levels",
        maps.len()
    );
    assert_eq!((checked, maps.len()), (6_024, 1_037));
}
