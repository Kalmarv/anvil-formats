# Changelog

## 0.3.0 (2026-11-06)

- `shaunwhite::shaders::read` reads the compressed SDB2 databases too (`ekshadersPC{low,med,high}compress.bin`,
  `ekshadersinitpccompress.bin`), which it refused: the byte 0xF5, the uncompressed size, and one streamed LZO2A data
  block. `shaders::decompress` gives the uncompressed database; each quality level's decodes to its uncompressed
  sibling byte for byte, and the init database, which has no sibling, reads (62 sources, 16 records).
  `data::read_stream` reads the streamed block form (each chunk's marker, sizes and checksum before its bytes, no chunk
  table). New error variants `data::Error::Marker` and `shaders::Error::Compressed`.
- tests/sws_shaders.rs: every shader database of the install reads; a compressed database laid out by hand and its
  refusals.

## 0.2.1 (2026-11-06)

- Depends on `dxcontainer` 0.5.0 (from crates.io, instead of a git revision), for a crate that uses dx 0.5.0 to
  hold one copy of it.

## 0.2.0 (2026-11-06)

- `rider::RiderSlot::colours` names the colour table (`HAIR_COLOURS`, `BEARD_COLOURS`) whose rows are the variants of the hair and beard activator slots; it is `None` for the other slots.
- `shaunwhite::texture` reads Shaun White Skateboarding TextureMaps strictly (`StoredTextureMap::parse`) and slices their mip levels by face and level (`levels`); `storage` and `d3d_format` give a format code's block size and the Direct3D 9 format the game creates it with.
- `shaunwhite::mesh` reads Shaun White Skateboarding Meshes: their MeshBones, the inline CompiledMesh's vertex streams, 16-bit indices, both draw-range partitions and bone palettes (`CompiledMesh::palettes`), the vertex layout the game declares (`CompiledMesh::layout`) and the material ids. It also reads the raw SubMeshes some meshes keep (`StoredMesh::sub_meshes`): each SubMesh's indices, per-triangle runs, positions, normals, colours, UV channels (`UvChannel`), morph channels (`MorphChannel`), material id and the copy it owns, so every Mesh of the game's data forges reads.
- Breaking: `shaunwhite::mesh::Error::SubMeshes` is removed, since a mesh with SubMeshes now reads; a SubMesh copy nested too deep is `Error::Depth`.
- `shaunwhite::skeleton` reads Shaun White Skateboarding Skeletons: the bones with their parents and bind pose (global and local position and rotation), orientation constraints, pose groups, IK and centre-of-mass data and mirror pairs; every Skeleton of the game's data forges reads, and its bind pose composes parent to child.
- `shaunwhite::animation` reads Shaun White Skateboarding Animations and AnimSets: a clip's length, bone mappings and tracks with their key times (`Track::seconds`) and values (`Track::values`: rotations in 48 and 64 bits and as f32, translations packed in 32 bits and as i16 millimetres), and the sets' clip pairs; every Animation and AnimSet of the game's data forges reads. The game's own skinned draws confirm the 64-bit and f32 rotations and both translations; the 48-bit rotations (kind 4) are not confirmed that way, since no traced frame is a kind 4 clip's pose. Their layout is told from the data only: the two bits that give the left-out component are the other way round from Riders Republic's (bit 15 of the first word is the low bit), the only order in which consecutive keys stay smooth where that component changes. Before this, keys that leave out y or z (11% of first keys) decoded as the other.
- `animation`'s 48-bit, 64-bit and f32 rotation decoders are shared with `shaunwhite::animation`. The 48-bit one takes the game's index bit order; Riders Republic's results are unchanged, and its order for indices 1 and 2 is now documented as unverified.
- Tests: `sky` compares `SkyKeyframe::radiance` and `StoredSky::radiance` with a reference copy of the shading (1e-5 relative). The skinned-draw tests now assert which traced frames and meshes a clip reproduces or leaves at rest, and say why the rest can't match. The Animation census pins each kind's first keys at the bind pose and the smoothness of kind 4 keys across a change of index.
- Tests: an `oracles.runtime` set compares both readers with what the game's own renderer uploaded and declared, and the Skeleton and mesh readers with the bone matrices of its skinned draws (`skin.tsv`: child bones sit at their stored local positions with the MeshBone matrices as the inverse bind, and a stored clip's decoded rotations and translations are the traced poses of four frames).
- The README is rewritten; the real-data test setup moved to CONTRIBUTING.md.

## 0.1.2 (2026-10-29)

- Depends on `dx` 0.4.0 and primase-foundations 0.3.0, neither of which has a type in this crate's API.
- Tests: Shaun White's real-data expectations follow the playable copy, the patched build.

## 0.1.1 (2026-10-27)

- Depends on `dx` 0.2.0 (`dxcontainer`, used inside the crate only), so a build that holds `dx` 0.2 has one copy of it.

## 0.1.0 (2026-10-27)

- First release: Ubisoft Anvil `.forge` archives and the meshes, textures, materials, entities and terrain in them, as Riders Republic and Shaun White Skateboarding store them.
