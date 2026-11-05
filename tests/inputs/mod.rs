//! The real-data inputs this crate's tests read, each with what it is and where it comes from. `gate: true` marks an
//! input that a `[run] require = 'gate'` configuration makes a failure, not a skip, when it is missing. Declared with
//! `primase_testdata::inputs!`; README.md here lists them. Integration tests reach it with `mod inputs;`, unit tests
//! with `#[cfg(test)] #[path = "../tests/inputs/mod.rs"] mod inputs;` in the crate root, and
//! tests/real_data_inputs.rs checks that every query the crate's sources write is declared here. Declare an input
//! here when a test first asks for it.

primase_testdata::inputs! {
    games {
        Riders = "riders",
        ShaunWhite = "shaunwhite",
    }
    installs {
        Riders {
            about: "Riders Republic's folder of `.forge` archives (Steam build 25394987)",
            source: "the user's own install",
            gate: false,
        },
        ShaunWhite {
            about: "Shaun White Skateboarding's (PC, 2010) folder of `.forge` archives, version 26: the playable copy, the patched build (executable of 2011-08-23, forges of 2010-10-27), 40 forges",
            source: "the user's own playable copy, the patched build: 33 data forges (23 of them dated 2010-10-27 and not the 1.0 release's bytes, 10 byte-identical to the release's of 2010-10-19) and the seven `DataPC_StreamedSounds*.forge` the 1.0 release's folder lacks; the forges' entry counts, sizes and block checksums read by a separate Python reading",
            gate: false,
        },
    }
    keys {}
    tools {}
    oracle_sets {
        Runtime = "runtime" {
            about: "what a game's own renderer creates and uploads when it loads the game's files: texture formats and level bytes, vertex declarations and buffer bytes, keyed to the forge objects they came from",
            source: "a resource trace of the game's own loader and renderer (the PC executable's code, run unchanged) on the user's own install, its uploads matched byte for byte to the objects; tables of hashes and declarations only, no game bytes",
        },
    }
    declared {
        Query::Install(Game::Riders, Some("DataPC_MAD_USNAT.forge")) => {
            status: Live,
            gate: false,
            about: r#"a large Anvil forge of version 27 (Riders Republic, Steam build 25394987: `DataPC_MAD_USNAT.forge`, 15,650,919,308 bytes, 767,463 entries; its tables are read, about 163 MB); tests/mesh_shape.rs, and the shape and mesh tests of the Anvil world builder (https://github.com/Kalmarv/primase/tree/main/worlds/anvil/tests)"#,
            source: r#"the user's own install; two MeshShape bodies' prefixes, section runs and tails hand-checked against a separate read-only decode of the file"#,
        },
        Query::Install(Game::Riders, Some("DataPC_MAD_WhiteRoom.forge")) => {
            status: Live,
            gate: false,
            about: r#"the base forge of installs.riders/DataPC_MAD_WhiteRoom_patch_01.forge's patch (Riders Republic, Steam build 25394987: `DataPC_MAD_WhiteRoom.forge`, 8,951,822 bytes, 388 entries); tests/data.rs"#,
            source: r#"the user's own install; two resource blocks' sizes, checksums, names and class ids hand-checked against a separate read-only decode of the file"#,
        },
        Query::Install(Game::Riders, Some("DataPC_MAD_WhiteRoom_patch_01.forge")) => {
            status: Live,
            gate: false,
            about: r#"one Anvil forge archive of version 27 (Riders Republic, Steam build 25394987: `DataPC_MAD_WhiteRoom_patch_01.forge`, 1,021,872 bytes, 337 entries); tests/forge.rs"#,
            source: r#"the user's own install; rows hand-checked against a separate read-only census of the file"#,
        },
        Query::Install(Game::ShaunWhite, Some("DataPC.forge")) => {
            status: Live,
            gate: false,
            about: r#"one Anvil forge of version 26 (Shaun White Skateboarding, PC patched build: `DataPC.forge` of 2010-10-27, 19,661,097 bytes, 11 entries: the game's bootstrap, settings and Russian localization), its tables, FILEDATA headers and every entry's data blocks (LZO2A) read; tests/forge.rs, data.rs"#,
            source: r#"the user's own playable copy; the first two rows (GlobalMetaFile, Graphic Bootstrap) and the second's two blocks hand-checked in a hex dump and by a separate Python reading of the tables and blocks, and AnvilToolkit 1.3.7's version-25 forge reader and AC2 compression table as the third-party reading of the layout"#,
        },
        Query::Install(Game::ShaunWhite, Some("DataPC_01.forge")) => {
            status: Live,
            gate: true,
            about: r#"Shaun White Skateboarding level-01 forge (version 26; the same bytes in the 1.0 release and the patched build): every texture in all 253 GridCellDataBlock entries; focused collision shape and inert-component fixtures in four cells; level placements, materials, collision and stored water flags, harbour tint and placement; 110 omni and 79 spot placements plus the root SkyLight and sun; restored metro mural and CliffDry layer weights; exact metro graffiti wall key for the REF-backed albedo derivation; the Shaun White Skateboarding world builder's tests (https://github.com/Kalmarv/primase/tree/main/worlds/shaunwhite/tests)"#,
            source: r#"the user's own copy, extracted unchanged; object directory/envelopes hand-checked separately, texture fields cross-checked with AnvilToolkit 1.3.7's AC2/Brotherhood readers and AC2 schema, duplicated compiled metadata and exact mip lengths checked on every texture; BC1/2/3 decoded against third-party bcdec_rs 0.2.0; code 12's BC3 storage established by sizes and normal-map content, not the AC2 enum; collision fields cross-checked with the AC2 schema and hand-inspected inert and multi-inert entity/shape references; light fields checked with the AC2 schema and exact body ends, SkyLight fields with native property descriptors and serializer; physical light units, colour encoding and direction signs remain approximations of the stored values; restored layer equation from the shipped SDB2 HLSL and Microsoft-disassembled SM3 pixel shader, native material property hashes identify layer tints and alpha switches"#,
        },
        Query::Oracle(OracleSet::Runtime, Game::ShaunWhite, "textures.tsv") => {
            status: Live,
            gate: false,
            about: r#"every texture level the game uploaded on its New Game route through the start of the level-02 load whose bytes equal a stored TextureMap level: forge, entry, object id, face, mip, width, height, the Direct3D format the game created the texture with and the SHA-256 of the uploaded bytes (1,037 TextureMaps, 6,024 levels); tests/sws_texture.rs"#,
            source: r#"a resource trace of the PC patched build's executable run on installs.shaunwhite, every upload's bytes hashed at Unlock and matched to the levels of every TextureMap of the 33 data forges"#,
        },
        Query::Oracle(OracleSet::Runtime, Game::ShaunWhite, "meshes.tsv") => {
            status: Live,
            gate: false,
            about: r#"every vertex and index buffer the game uploaded on the same route whose bytes equal a stored mesh's vertex stream or index bytes: forge, entry, object id, flags, stride, which part, the SHA-256 of the uploaded bytes, and for vertex buffers the vertex declaration, stream stride and offset of each draw that bound it (516 meshes); tests/sws_mesh.rs"#,
            source: r#"the same resource trace: each buffer's bytes hashed at Unlock, each distinct draw's declaration and stream bindings logged, matched to every Mesh of the 33 data forges"#,
        },
        Query::Oracle(OracleSet::Runtime, Game::ShaunWhite, "skin.tsv") => {
            status: Live,
            gate: false,
            about: r#"the bone matrices of every skinned draw the game made on the same route (40 draws of 6 meshes in frames 7199 to 7204): forge, entry, mesh id, frame, draw range, palette entry, the MeshBone's name CRC and the three constant registers of the vertex shader's `g_Bones` array that hold the bone (12 f32 as u32 hex); tests/sws_skeleton.rs"#,
            source: r#"the same resource trace: each skinned draw's vertex-shader float constants copied at the draw, its shader's constant table naming `g_Bones`, its vertex buffer matched to a stored mesh's stream and its index range to one of the mesh's draw ranges"#,
        },
    }
    debug {}
}
