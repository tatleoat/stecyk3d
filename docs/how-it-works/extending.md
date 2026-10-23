# Extending it

## Adding a primitive

1. Write the ray intersection in `src/geometry/` (return distance, point, normal, texture coordinates).
2. Add the shape to the scene reader (`scene_parts.rs` and friends) and to the description (`description.rs`) if Blender should be able to make it.
3. Add a case to `src/render/plan.rs` so the plan builds objects of it.
4. Add tests: component rays and a picture hash.

## Adding a scene-file part

Put the parser in a `scene_*.rs` file and extend `Scene`. Refuse unknown versions of the part with a named error instead of skipping bytes.

## Adding a shading rule

Rules live in `src/shading/` and are called from `src/render/lit.rs`. Keep the order of operations explicit and avoid fused multiply-add. Add the rule's inputs to the plain request struct so a staged (GPU) version could carry them.

## Adding an add-on feature

1. Add the setting in `props.py` and its name in `names.py`.
2. Read it in `gather.py`.
3. Convert it to the description in a pure module and test it with `unittest`.
4. Read the description field in `src/description.rs`.

## Guidelines

- Stages exchange plain `Copy` data; no references or boxes in per-ray structs.
- No recursion in the per-ray loop.
- No new crates without a good reason.
- Comments say why, not what.
- Do not guess: if a case is not supported, return an error that names it.
