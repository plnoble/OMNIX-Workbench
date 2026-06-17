# Errors And Lessons

## 2026-06-13 - Tauri full bundle failed at WiX MSI stage

- Symptom: `npm.cmd run tauri -- build` compiled the release app but failed while running WiX `light.exe` for MSI generation.
- Evidence: release exe was produced at `src-tauri/target/release/omnix-app.exe`; the command failed after `Running light to produce ... bundle/msi/omnix-app_0.1.0_x64_en-US.msi`.
- Root cause status: not fully isolated. The command output did not include a detailed WiX error, and elevated reruns were blocked by approval timeout.
- Safe workaround: run `npm.cmd run tauri -- build --bundles nsis`, which successfully produced `src-tauri/target/release/bundle/nsis/omnix-app_0.1.0_x64-setup.exe`.
- Follow-up: rerun full MSI packaging in a non-sandbox shell or with explicit elevated approval and capture full WiX `light.exe` stderr/log output.
