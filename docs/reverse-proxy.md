# Publishing LocalReel behind a reverse proxy

LocalReel binds `127.0.0.1` by default. For a NAS or a small server you either
reach it over the LAN directly or put it behind a reverse proxy that terminates
HTTPS. The proxy is the recommended option for anything beyond a trusted LAN.

## What the server expects

- Bind it somewhere the proxy can reach it. On the same host the default
  loopback bind is enough; in a container publish the port on `127.0.0.1` only.
- **Accounts are mandatory.** Every `/api/v1` route except health and the auth
  endpoints requires a signed-in account; the first run asks for the owner
  account in the browser.
- Send `X-Forwarded-Proto: https`. The server reads it to mark the session and
  CSRF cookies `Secure`, so they are not sent over plain HTTP.
- Keep the port itself off untrusted networks: the server speaks plain HTTP.

## Caddy

```caddyfile
reel.example.test {
    reverse_proxy 127.0.0.1:3001
}
```

Caddy sets `X-Forwarded-Proto` itself and handles certificates.

## nginx

```nginx
server {
    listen 443 ssl http2;
    server_name reel.example.test;

    # Backup restores can upload a large file.
    client_max_body_size 512m;

    location / {
        proxy_pass http://127.0.0.1:3001;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;

        # Job progress is server-sent events, and media is streamed with byte
        # ranges: buffering would delay both.
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}
```

## Notes

- `/api/v1/events` is an SSE stream; without `proxy_buffering off` the job
  progress panel only updates when a buffer flushes.
- Range requests for playback pass through unchanged: do not enable response
  buffering or compression for `/api/v1/media/`.
- Uploads to `/api/v1/backup/restore/upload` are limited to 512 MiB by the
  server; keep the proxy limit at or above that.
- If you change the bind address, remember `LOCALREEL_ALLOW_REMOTE=1` (or
  `[server] allow_remote = true`): the server refuses a non-loopback bind
  without it, because that used to mean "no authentication".
