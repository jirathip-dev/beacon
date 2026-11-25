# `beacon serve --notify`

With `--notify <path>`, serve also watches its own checks and every other host in `hosts` (it polls each
host's `url` every 60 s) and POSTs signed webhooks for changes: a check that is up or down on 2 polls in a
row, or a host that failed 3 polls in a row (and its first good poll after that). Changes within 30 s of the
first one go out as one batch (`{"kind":"batch","summary":…,"changes":[…],"at":…}`); a down host's checks
are left out while it is down. Something that stays down is reminded (`still down: <host>/<check> for <n>h`)
every `notify.remind_every_h` hours, at most `notify.remind_max` times (top-level `"notify":
{"remind_every_h": 4, "remind_max": 3}`, these defaults when absent; `remind_max: 0` turns reminders off).
Nothing is sent at start or on a reload.

The notify file is `{"url": "https://…/hooks/in/<id>", "secret": "…"}`, conventionally
`~/.config/beacon/notify.json`, mode 600 (serve exits 2 on a file group or others can read, a missing file or
an invalid one). Each POST carries `X-Signature-256: sha256=<hex HMAC-SHA256 of the body under the secret>`.
The secret is never logged. `SSL_CERT_FILE`, when set, replaces the built-in roots for these requests.
As a dead-man's switch serve also POSTs `{"kind":"alive","host":…,"check":null,"status":"up","message":"<n>
hosts, <m> down","at":…}`, signed the same way, every `"alive_every_s"` seconds of the notify file (default
300, at least 60, `0` turns it off). It is tried once, never retried, and a failure is logged once per outage;
alerting when no `alive` arrives for 15 min is the receiver's job. The webhook is generic: any HTTPS receiver
that checks `X-Signature-256` (GitHub-style HMAC) works; a cooee webhook routine is the one in use.
