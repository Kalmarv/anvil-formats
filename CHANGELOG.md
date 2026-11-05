# Changelog

## Unreleased

- `rider::RiderSlot::colours` names the colour table (`HAIR_COLOURS`, `BEARD_COLOURS`) whose rows are the variants of the hair and beard activator slots; it is `None` for the other slots.
- `shaunwhite::texture` reads Shaun White Skateboarding TextureMaps strictly (`StoredTextureMap::parse`) and slices their mip levels by face and level (`levels`); `storage` and `d3d_format` give a format code's block size and the Direct3D 9 format the game creates it with.
- `shaunwhite::mesh` reads Shaun White Skateboarding Meshes: their MeshBones, the inline CompiledMesh's vertex streams, 16-bit indices, both draw-range partitions and bone palettes (`CompiledMesh::palettes`), the vertex layout the game declares (`CompiledMesh::layout`) and the material ids. It also reads the raw SubMeshes some meshes keep (`StoredMesh::sub_meshes`): each SubMesh's indices, per-triangle runs, positions, normals, colours, UV channels (`UvChannel`), morph channels (`MorphChannel`), material id and the copy it owns, so every Mesh of the game's data forges reads.
- Breaking: `shaunwhite::mesh::Error::SubMeshes` is removed, since a mesh with SubMeshes now reads; a SubMesh copy nested too deep is `Error::Depth`.
- `shaunwhite::skeleton` reads Shaun White Skateboarding Skeletons: the bones with their parents and bind pose (global and local position and rotation), orientation constraints, pose groups, IK and centre-of-mass data and mirror pairs; every Skeleton of the game's data forges reads, and its bind pose composes parent to child.
- `shaunwhite::animation` reads Shaun White Skateboarding Animations and AnimSets: a clip's length, bone mappings and tracks with their key times (`Track::seconds`) and values (`Track::values`: rotations in 48 and 64 bits and as f32, translations packed in 32 bits and as i16 millimetres), and the sets' clip pairs; every Animation and AnimSet of the game's data forges reads.
- `animation`'s 48-bit, 64-bit and f32 rotation decoders are shared with `shaunwhite::animation`; their results are unchanged.
- Tests: an `oracles.runtime` set compares both readers with what the game's own renderer uploaded and declared, and the Skeleton and mesh readers with the bone matrices of its skinned draws (`skin.tsv`: child bones sit at their stored local positions with the MeshBone matrices as the inverse bind, and a stored clip's decoded rotations and translations are the traced poses of four frames).
- The README is rewritten; the real-data test setup moved to CONTRIBUTING.md.

## 0.1.2 (2026-10-29)

- Depends on `dx` 0.4.0 and primase-foundations 0.3.0, neither of which has a type in this crate's API.
- Tests: Shaun White's real-data expectations follow the playable copy, the patched build.

## 0.1.1 (2026-10-27)

- Depends on `dx` 0.2.0 (`dxcontainer`, used inside the crate only), so a build that holds `dx` 0.2 has one copy of it.

## 0.1.0 (2026-10-27)

- First release: Ubisoft Anvil `.forge` archives and the meshes, textures, materials, entities and terrain in them, as Riders Republic and Shaun White Skateboarding store them.
