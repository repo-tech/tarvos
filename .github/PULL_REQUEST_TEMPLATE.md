## Summary

<!-- What changed and why. Link the issue with "Fixes #123" if applicable. -->

## Type of change

- [ ] Compiler correctness fix (behavior-preserving)
- [ ] New compiler/native-subset feature
- [ ] CLI / gateway / installer
- [ ] Frontend / Studio
- [ ] Documentation
- [ ] CI / release

## Compatibility impact

- [ ] No change to generated-code behavior
- [ ] Backward-compatible behavior extension (previously-fallback code now compiles)
- [ ] Changed behavior for existing code (describe below — requires justification)

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo test --workspace` passes
- [ ] Added/updated a regression test that fails before this change
- [ ] `npm run build` passes (frontend changes)
- [ ] `docs/compiler-status.md` updated if the supported subset changed
- [ ] `CHANGELOG.md` updated for user-visible changes

## Semantic correctness notes

<!-- For compiler changes: how did you verify Python parity (differential
test against CPython, compatibility gate test, etc.)? -->

## Screenshots

<!-- UI changes -->
