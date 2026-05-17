# Trusted release keys

This directory holds the **public** part of the MCPix release signing key.
The matching **private** key lives only as:

- A GitHub Actions secret named `MCPIX_SIGN_PRIVKEY_HEX` (64-char hex of the
  32-byte Ed25519 seed).
- An offline backup held by the release manager.

It is never written to this repository.

## Files

- `release.pub` — 32 raw bytes, Ed25519 verifying key, embedded into
  `mcpix_core::signature::RELEASE_PUBKEY` via `include_bytes!`.

## Rotation

1. Run `cargo xtask gen-release-key` (refuses if `release.pub` exists; delete
   it first, intentionally).
2. The command prints the new private key in hex once. Copy into the CI
   secret `MCPIX_SIGN_PRIVKEY_HEX` and store the offline backup. The local
   copy at `target/release-key.priv` is in `.gitignore` — never commit.
3. Commit the new `release.pub` in a dedicated commit titled clearly with
   "release-key rotation" so it stands out in audit logs.
4. Build & publish a new release; older clients verifying with the previous
   pubkey will reject — communicate the rotation to consumers.

## DEV note

The key currently checked in was generated locally during the pre-deposit
phase. Before the first production release the team **must** rotate the key
on a hardened machine and replace this file in a clean commit.
