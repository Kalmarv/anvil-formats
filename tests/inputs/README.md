# Real-data inputs

What this crate's real-data tests read, as tests/inputs/mod.rs declares them: the keys of the configuration file
`GAME_DATA` names (its layout is documented in [primase-testdata](https://github.com/Kalmarv/primase-testdata)), the oracle sets and the files of each, and the debug keys
`GAME_DATA_DEBUG` sets. A test whose input is not configured skips with a line naming its key.

<!-- real-data inputs: made by primase_testdata::doc::tables from tests/inputs/mod.rs -->

#### Configuration keys

| key | gate | about | source |
| --- | --- | --- | --- |
| `installs.riders` | - | Riders Republic's folder of `.forge` archives (Steam build 25394987) | the user's own install |
| `installs.shaunwhite` | - | Shaun White Skateboarding's (PC, 2010) folder of `.forge` archives, version 26: the playable copy, the patched build (executable of 2011-08-23, forges of 2010-10-27), 40 forges | the user's own playable copy, the patched build: 33 data forges (23 of them dated 2010-10-27 and not the 1.0 release's bytes, 10 byte-identical to the release's of 2010-10-19) and the seven `DataPC_StreamedSounds*.forge` the 1.0 release's folder lacks; the forges' entry counts, sizes and block checksums read by a separate Python reading |
| `scratch.root` | - | where real-data tests write their outputs (the OS temporary folder when unset) | - |

#### Declared files

| key | status | gate | about | source |
| --- | --- | --- | --- | --- |
| `installs.riders/DataPC_MAD_USNAT.forge` | live | - | a large Anvil forge of version 27 (Riders Republic, Steam build 25394987: `DataPC_MAD_USNAT.forge`, 15,650,919,308 bytes, 767,463 entries; its tables are read, about 163 MB); tests/mesh_shape.rs, and the shape and mesh tests of the Anvil world builder (https://github.com/Kalmarv/primase/tree/main/worlds/anvil/tests) | the user's own install; two MeshShape bodies' prefixes, section runs and tails hand-checked against a separate read-only decode of the file |
| `installs.riders/DataPC_MAD_WhiteRoom.forge` | live | - | the base forge of installs.riders/DataPC_MAD_WhiteRoom_patch_01.forge's patch (Riders Republic, Steam build 25394987: `DataPC_MAD_WhiteRoom.forge`, 8,951,822 bytes, 388 entries); tests/data.rs | the user's own install; two resource blocks' sizes, checksums, names and class ids hand-checked against a separate read-only decode of the file |
| `installs.riders/DataPC_MAD_WhiteRoom_patch_01.forge` | live | - | one Anvil forge archive of version 27 (Riders Republic, Steam build 25394987: `DataPC_MAD_WhiteRoom_patch_01.forge`, 1,021,872 bytes, 337 entries); tests/forge.rs | the user's own install; rows hand-checked against a separate read-only census of the file |
| `installs.shaunwhite/DataPC.forge` | live | - | one Anvil forge of version 26 (Shaun White Skateboarding, PC patched build: `DataPC.forge` of 2010-10-27, 19,661,097 bytes, 11 entries: the game's bootstrap, settings and Russian localization), its tables, FILEDATA headers and every entry's data blocks (LZO2A) read; tests/forge.rs, data.rs | the user's own playable copy; the first two rows (GlobalMetaFile, Graphic Bootstrap) and the second's two blocks hand-checked in a hex dump and by a separate Python reading of the tables and blocks, and AnvilToolkit 1.3.7's version-25 forge reader and AC2 compression table as the third-party reading of the layout |
| `installs.shaunwhite/DataPC_01.forge` | live | yes | Shaun White Skateboarding level-01 forge (version 26; the same bytes in the 1.0 release and the patched build): every texture in all 253 GridCellDataBlock entries; focused collision shape and inert-component fixtures in four cells; level placements, materials, collision and stored water flags, harbour tint and placement; 110 omni and 79 spot placements plus the root SkyLight and sun; restored metro mural and CliffDry layer weights; exact metro graffiti wall key for the REF-backed albedo derivation; the Shaun White Skateboarding world builder's tests (https://github.com/Kalmarv/primase/tree/main/worlds/shaunwhite/tests) | the user's own copy, extracted unchanged; object directory/envelopes hand-checked separately, texture fields cross-checked with AnvilToolkit 1.3.7's AC2/Brotherhood readers and AC2 schema, duplicated compiled metadata and exact mip lengths checked on every texture; BC1/2/3 decoded against third-party bcdec_rs 0.2.0; code 12's BC3 storage established by sizes and normal-map content, not the AC2 enum; collision fields cross-checked with the AC2 schema and hand-inspected inert and multi-inert entity/shape references; light fields checked with the AC2 schema and exact body ends, SkyLight fields with native property descriptors and serializer; physical light units, colour encoding and direction signs remain approximations of the stored values; restored layer equation from the shipped SDB2 HLSL and Microsoft-disassembled SM3 pixel shader, native material property hashes identify layer tints and alpha switches |

<!-- end of real-data inputs -->
