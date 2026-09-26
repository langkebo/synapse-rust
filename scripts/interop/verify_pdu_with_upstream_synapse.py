#!/usr/bin/env python3
"""Cross-implementation gate for U-13: verify one of *our* PDUs with upstream Synapse.

Why this exists
---------------
A live `/send_join` + `/send` run against a peer Synapse is the ideal interop
gate, but it needs two resolvable server names and TLS certificates both sides
trust.  This sandbox cannot provide that: `/etc/hosts` is not writable, `sudo` is
blocked, `*.localhost` does not resolve here, Docker Hub is unreachable, and this
repository's federation client has no custom-CA / skip-verification knob.  What
*is* available is a real `matrix-synapse==1.161.0` (the behavioural baseline this
repo targets) installed in a venv.

So this script closes the same question with Synapse's **own implementation**
instead of the wire: it takes a PDU our code produced, and re-derives, using
upstream's redaction / canonical JSON / ed25519 verification:

  1. `hashes.sha256`            — `synapse.crypto.event_signing.compute_content_hash`
  2. the event ID               — `redact_event_dict` → canonical JSON → SHA-256 →
                                   unpadded Base64 (URL-safe for v4+, standard for v3)
  3. the server signature       — `signedjson.sign.verify_signed_json` over the
                                   redacted event

Any mismatch exits non-zero.  If this passes, a peer running Synapse accepts the
same bytes we emit; the only thing left unproven is transport.

Usage
-----
    /tmp/peer-synapse/bin/python scripts/interop/verify_pdu_with_upstream_synapse.py \
        tests/interop/fixtures/local_pdu_v10.json

Fixture shape (see tests/interop/fixtures/README.md):
    {
      "produced_by": "…",              # how to regenerate it
      "room_version": "10",
      "signing_server_name": "example.com",
      "signing_key_id": "ed25519:1",   # "<alg>:<version>"
      "signing_seed_base64": "…",      # 32-byte ed25519 seed, test key only
      "event_id": "$…",                # what our code assigned
      "pdu": { … }                     # exactly what we would send (no event_id for v3+)
    }
"""
from __future__ import annotations

import hashlib
import json
import sys

from canonicaljson import encode_canonical_json
from signedjson.key import decode_signing_key_base64, get_verify_key
from signedjson.sign import verify_signed_json
from synapse.api.room_versions import KNOWN_ROOM_VERSIONS
from synapse.crypto.event_signing import compute_content_hash
from synapse.synapse_rust.events import redact_event_dict
from unpaddedbase64 import encode_base64


def _fail(message: str) -> None:
    print(f"FAIL: {message}")
    sys.exit(1)


def main() -> None:
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)

    fixture_path = sys.argv[1]
    with open(fixture_path, encoding="utf-8") as handle:
        fixture = json.load(handle)

    room_version_str = str(fixture["room_version"])
    if room_version_str not in KNOWN_ROOM_VERSIONS:
        _fail(f"this Synapse does not know room version {room_version_str!r}")
    room_version = KNOWN_ROOM_VERSIONS[room_version_str]
    pdu = fixture["pdu"]
    claimed_event_id = fixture["event_id"]
    server_name = fixture["signing_server_name"]

    # ── 0. shape: v3+ PDUs must not carry an event_id or a user_id ──────────
    if int(room_version_str) >= 3:
        for forbidden in ("event_id", "user_id"):
            if forbidden in pdu:
                _fail(f"v{room_version_str} PDU must not contain {forbidden!r}: {pdu.get(forbidden)!r}")

    # ── 1. content hash ─────────────────────────────────────────────────────
    name, digest = compute_content_hash(dict(pdu), hashlib.sha256)
    upstream_hash = encode_base64(digest)
    ours = pdu.get("hashes", {}).get("sha256")
    if upstream_hash != ours:
        _fail(f"hashes.sha256 mismatch: upstream={upstream_hash} ours={ours}")
    print(f"OK   content hash  sha256={ours}")

    # ── 2. reference-hash event ID ──────────────────────────────────────────
    redacted = redact_event_dict(room_version, dict(pdu))
    for drop in ("signatures", "unsigned", "age_ts"):
        redacted.pop(drop, None)
    reference = hashlib.sha256(encode_canonical_json(redacted)).digest()
    # v3 used the standard Base64 alphabet; v4+ the URL-safe one (spec room v4).
    upstream_event_id = "$" + encode_base64(reference, urlsafe=int(room_version_str) >= 4)
    if upstream_event_id != claimed_event_id:
        _fail(f"event id mismatch: upstream={upstream_event_id} ours={claimed_event_id}")
    print(f"OK   event id      {claimed_event_id}")

    # ── 3. signature over the redacted event ────────────────────────────────
    algorithm, _, version = fixture["signing_key_id"].partition(":")
    signing_key = decode_signing_key_base64(algorithm, version, fixture["signing_seed_base64"])
    verify_key = get_verify_key(signing_key)
    to_verify = redact_event_dict(room_version, dict(pdu))
    for drop in ("unsigned", "age_ts"):
        to_verify.pop(drop, None)
    try:
        verify_signed_json(to_verify, server_name, verify_key)
    except Exception as error:  # signedjson raises a plain Exception subclass
        _fail(f"signature verification failed: {error!r}")
    print(f"OK   signature     {server_name} {fixture['signing_key_id']}")

    print(f"PASS {fixture_path}: upstream Synapse accepts this PDU's hash, event id and signature")


if __name__ == "__main__":
    main()
