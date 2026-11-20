import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function verifyNsisPayload(built, installed) {
  // Tauri CLI 2.11.4 patches the first UNK marker for the NSIS payload, then
  // restores the original build output. Compare every byte against that exact
  // transformation, without normalizing arbitrary markers in the installed EXE.
  // https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-bundler/src/bundle.rs
  const originalMarker = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK');
  const nsisMarker = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_NSS');
  const offset = built.indexOf(originalMarker);
  if (offset < 0) throw new Error('Built candidate has no original Tauri bundle marker');
  const expected = Buffer.from(built);
  nsisMarker.copy(expected, offset);
  if (!expected.equals(installed)) {
    throw new Error('Installer application differs beyond the expected Tauri NSIS marker patch');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [, , builtPath, installedPath] = process.argv;
  if (!builtPath || !installedPath) throw new Error('Usage: verify-nsis-payload.mjs BUILT_EXE INSTALLED_EXE');
  verifyNsisPayload(readFileSync(builtPath), readFileSync(installedPath));
  console.log('Installed NSIS application matches the built candidate');
}
