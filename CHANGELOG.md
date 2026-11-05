# Changelog

## Unreleased

- `rider::RiderSlot::colours` names the colour table (`HAIR_COLOURS`, `BEARD_COLOURS`) whose rows are the variants of the hair and beard activator slots; it is `None` for the other slots.
- `shaunwhite::texture` reads Shaun White Skateboarding TextureMaps strictly (`StoredTextureMap::parse`) and slices their mip levels by face and level (`levels`); `storage` and `d3d_format` give a format code's block size and the Direct3D 9 format the game creates it with.
- `shaunwhite::mesh` reads Shaun White Skateboarding Meshes: their MeshBones, the inline CompiledMesh's vertex streams, 16-bit indices, both draw-range partitions and bone palettes (`CompiledMesh::palettes`), the vertex layout the game declares (`CompiledMesh::layout`) and the material ids. It also reads the raw SubMeshes some meshes keep (`StoredMesh::sub_meshes`): each SubMesh's indices, per-triangle runs, positions, normals, colours, UV channels (`UvChannel`), morph channels (`MorphChannel`), material id and the copy it owns, so every Mesh of the game's data forges reads.
- Tests: an `oracles.runtime` set compares both readers with what the game's own renderer uploaded and declared.
- The README is rewritten; the real-data test setup moved to CONTRIBUTING.md.

## 0.1.2 (2026-10-29)

- Depends on `dx` 0.4.0 and primase-foundations 0.3.0, neither of which has a type in this crate's API.
- Tests: Shaun White's real-data expectations follow the playable copy, the patched build.

## 0.1.1 (2026-10-27)

- Depends on `dx` 0.2.0 (`dxcontainer`, used inside the crate only), so a build that holds `dx` 0.2 has one copy of it.

## 0.1.0 (2026-10-27)

- First release: Ubisoft Anvil `.forge` archives and the meshes, textures, materials, entities and terrain in them, as Riders Republic and Shaun White Skateboarding store them.
