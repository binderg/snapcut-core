# GL Transitions pack

125 transitions from [gl-transitions](https://github.com/gl-transitions/gl-transitions),
converted from GLSL to the engine's WGSL transition contract. An effect pack:
no program, only `transitions/<name>/` folders (`effect.json`,
`transition.wgsl`, and images for the few that need one).

- Ids are `gl.transition/<name>` (`fx.list` shows them as `origin: "pack"`).
  The first time a project uses one, the engine copies its folder into the
  project's `effects/`, so the project renders without the pack.
- Licenses: the repository is MIT (`LICENSE`); a few shaders have their own
  (BSD). Every `transition.wgsl` starts with its shader's original author
  and license header. Keep them when editing.
- Install: the desktop editor installs this pack on start. From the CLI:
  `ve plugin install plugins/gl-transitions`.
- Regenerate: the converter is the editor's `scripts/import_gl_transitions.py`
  (needs `naga-cli` 30.0.1 and a clone of the gl-transitions repository). It
  rewrites `transitions/` and keeps this README. Bump the version with
  `--pack-version` when shaders change, so the editor reinstalls it.
