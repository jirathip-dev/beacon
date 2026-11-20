## Summary

Decision: The phone keeps probing the http/tcp monitors for the live row, and one host named in the config (`"prober"`, the Mini) probes them too, every 60 s, to keep their history. Every serve keeps the up/down changes of everything in its `/status` (checks, and #41's rules and #39's jobs with no work of theirs) in one small JSON file, and the prober keeps response times in one fixed 5.8 KB file per monitor: 24 h per probe plus 30 days per hour. The app reads `/history` (changes; the ETag holds until the next change) and `/history/<id>` (one monitor's times) only while a detail screen is open. A certificate that stops being valid within 14 days makes the monitor down on both sides.

Slices: (1) the history shapes, schemas and bound tests; (2) the store on disk, the tap in serve and `/history` for checks; (3) the prober: `prober` and `cert_warn_days` in the config, http/tcp/cert probes, the time rings, `/history/<id>`, the budget test with 20 monitors; (4) after design, the app: uptime, incident log, response-time chart, the certificate rule.

Open questions: (1) The row's colour stays the phone's own probe and the history is the Mini's view; say if the Mini should be the one truth instead (the app would stop probing). (2) The Mini does not send monitor changes to cooee in this issue; say if it should. (3) `rustls` becomes a direct dependency for the certificate check; it is already compiled in as ureq's TLS, so no new crate is downloaded.

# History, TLS expiry and the prober (issue #42)

**To review, read two things:** `history.md` (how the callers read, the shapes, the module boundaries, the
statecharts, the tests) and `adr.md` (H1–H8: who probes what, the store, the endpoints, the certificate
rule, the budget and the disk use, each with the options turned down). The wire contracts are
`history.schema.json` (`GET /history`, and the store's `events.json`) and `times.schema.json`
(`GET /history/<id>`).

This folder amends D3, D4, D5, D7 and D8 of `../beacon/adr.md`. It is its own folder so that it shares no
file with the architecture work of #39 and #41, which are open at the same time.

## The numbers behind it, measured on the Mini

| | |
|---|---|
| serve today (`cargo test --test budget` at 3616a81) | 0.069 % of a core, 2.49 MB peak, 45.6 wake-ups an idle minute |
| one fresh HTTPS probe (ureq 3.5.2, rustls 0.23.46, local server, 2 000 in a row) | 0.2 ms CPU |
| peak memory, probes one after another / 4 at once / 20 at once | 2.5 / 3.0 / 5.2 MB with 16 KB buffers; 3.5 / 5.7 / 17.2 MB with ureq's default |
| a handshake with the clock 14 days ahead / far ahead | passes / fails with the certificate's exact `notAfter` |
| disk for 20 monitors over 30 days | about 140 KB, never over 1 MB |

The probe numbers come from a throwaway program outside the repo; nothing of it is committed.

## Slices, in commit order

1. **Shapes** (pure): `history.rs` (`History`, `Track`, `Incident`, `MonitorId`, `Times`), the two schemas'
   examples `docs/history.example.json` and `docs/times.example.json`, the golden and bound tests.
2. **Store and tap** (every host): `events.json` load, write and alive stamp in `state.rs`, the 30-day sweep,
   `record` in `serve.rs`, the tick, `GET /history`, the restart test.
3. **Prober** (the `prober` host): `prober` and `cert_warn_days` in `config.rs` and
   `../beacon/monitors.schema.json`, `net.rs` (the agent moves out of `notify.rs`), `monitor.rs` (http, tcp,
   certificate), `prober.rs`, the ring files, `GET /history/<id>`, the budget test with 20 monitors, the
   repo README.
4. **App**, after the design is approved: `History.swift` (bodies, uptime, fetch), the certificate rule in
   `Refresher.http()`, the screens, verify at 390.

Design can start from `history.md`'s "The app" sketch and the two example bodies in "How the callers read".
