# Changelog

## Unreleased

- `rider::RiderSlot::colours` names the colour table (`HAIR_COLOURS`, `BEARD_COLOURS`) whose rows are the variants of the hair and beard activator slots; it is `None` for the other slots.

## 0.1.2 (2026-10-29)

- Depends on `dx` 0.4.0 and primase-foundations 0.3.0, neither of which has a type in this crate's API.
- Tests: Shaun White's real-data expectations follow the playable copy, the patched build.

## 0.1.1 (2026-10-27)

- Depends on `dx` 0.2.0 (`dxcontainer`, used inside the crate only), so a build that holds `dx` 0.2 has one copy of it.

## 0.1.0 (2026-10-27)

- First release: Ubisoft Anvil `.forge` archives and the meshes, textures, materials, entities and terrain in them, as Riders Republic and Shaun White Skateboarding store them.
