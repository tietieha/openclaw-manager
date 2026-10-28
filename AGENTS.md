# Build Logs

- Write all local build, rebuild, packaging, and build-check logs under
  `logs/build/`, never in the repository root.
- Create the directory before redirecting command output.
- For background builds, use absolute paths under `logs/build/` for both
  `RedirectStandardOutput` and `RedirectStandardError`.
- Keep descriptive filenames such as `tauri-rebuild-autostart.log` and
  `tauri-rebuild-autostart.err.log`.
- These are local generated files and must not be committed.
- Do not relocate OpenClaw runtime logs from its user configuration directory.

# Build Artifacts

- Put locally distributable Windows binaries and installers under
  `artifacts/windows/`. Do not place generated files in the repository root.
- Keep Cargo build caches and intermediate bundles in `src-tauri/target/`.
- Use `build-windows.bat --no-pause` for local Windows packaging. It copies
  successful build outputs into `artifacts/windows/`.
- Verify build completion and artifact timestamps before reporting success.
