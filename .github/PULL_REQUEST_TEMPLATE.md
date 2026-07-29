## What & why

<!-- One or two sentences. Link the issue if one exists: Fixes #NN -->

## Checklist

From [CONTRIBUTING.md](../CONTRIBUTING.md):

- [ ] `npx tsc --noEmit` passes
- [ ] `cargo test` passes (`cd src-tauri`)
- [ ] `cargo clippy --all-targets -- -D warnings` is clean
- [ ] No version-bump changes unless this PR is a release
- [ ] New Tauri commands registered in `lib.rs` `invoke_handler!` and wrapped in `src/lib/tauri.ts`
- [ ] No secrets or tokens in the diff
- [ ] Docs updated if documented behavior changed

## Testing done

<!-- How you verified: OS, provider(s) tested against, what you clicked/ran -->
