# Touchpad

A read-only Bevy/WASM dashboard for `world`. It displays the coordinate map,
humans, positions, movement, owners, and total/player/dashboard connection counts.
UDP spectators count as players; WebTransport connections count as dashboards.
Dashboards never claim a human or send movement input.

## Local development

Install [Trunk](https://trunkrs.dev/) and the WASM target:

```sh
cargo install trunk --locked
rustup target add wasm32-unknown-unknown
```

From the workspace root, start world first:

```sh
cargo run -p world -- --webtransport --advertise-ip 127.0.0.1
```

Then, in another terminal:

```sh
cd touchpad
trunk serve
```

Open `http://127.0.0.1:8080` in a browser supporting WebTransport datagrams and
`serverCertificateHashes`. Localhost HTTP is a browser secure context. There is
no WebSocket fallback. To move a human, run `cargo run -p observer` in another
terminal and use WASD in its window.

World listens on UDP `5000` for players and UDP `5001` for WebTransport/HTTP3.
Both transports share a maximum of ten connected clients.

## Local network

Replace `192.168.1.20` with the server computer's LAN IPv4 address:

```sh
cargo run -p world -- --webtransport --bind 0.0.0.0 --advertise-ip 192.168.1.20
```

The dashboard page must use trusted HTTPS when opened from another device.
Install [mkcert](https://github.com/FiloSottile/mkcert), then create a local CA and
a page certificate on the server computer:

```sh
cd touchpad
mkdir certs
mkcert -install
mkcert -cert-file certs/page.pem -key-file certs/page-key.pem 192.168.1.20 localhost 127.0.0.1
trunk serve --address 0.0.0.0 --tls-cert-path certs/page.pem --tls-key-path certs/page-key.pem
```

Install the CA's **public** `rootCA.pem` on each accessing device, enabling full
trust where required by its OS. Locate it using `mkcert -CAROOT`. Keep the CA
private key and page private key on the server. Open
`https://192.168.1.20:8080` on the other device. Permit inbound TCP `8080` and UDP
`5001` in the server's firewall. No automatic firewall or trust-store changes
are made by this project.

World generates a separate short-lived ECDSA WebTransport certificate using
renet2's certificate helper. Its SHA-256 hash pins the transport certificate;
it does not make the dashboard page HTTPS trusted. Browser and device support
for WebTransport and certificate hashes varies; unsupported devices show an
error in the dashboard. Application authentication is not implemented: use this
example on a trusted LAN.

## Tailscale on Nest

Open `http://nest:8080` from another device on the tailnet. The entry page sends
you to `https://nest.bunny-hamlet.ts.net`, where Tailscale Serve supplies trusted
HTTPS for the dashboard. Use the full HTTPS name when bookmarking the dashboard.

Start world from the workspace root with Nest's Tailscale IPv4 address:

```sh
cargo run -p world -- --webtransport --bind 0.0.0.0 --advertise-ip 100.65.143.33
```

In another terminal, start `trunk serve` inside `touchpad`. Keep its default
loopback listener on port 8080. From the workspace root, run the entry page
server in another terminal:

```sh
python3 -m http.server 8081 --bind 127.0.0.1 --directory touchpad/tailscale-entry
```

Configure the two tailnet entry points from the workspace root:

```sh
tailscale serve --bg --https=443 http://127.0.0.1:8080
tailscale serve --bg --http=8080 http://127.0.0.1:8081
```

Tailscale's port 8080 listener uses the tailnet interface; Trunk's port 8080
listener uses loopback. The HTTPS page connects directly to
`100.65.143.33:5001` over WebTransport, so tailnet access rules must allow TCP
443/8080 and UDP 5001. The page proxy does not forward the UDP transport.
After restarting world, wait for Trunk's build to finish or copy the fresh
`connection.json` into `dist`, then reconnect. Tailscale Serve's background
configuration persists, but world, Trunk, and the entry page server still need
to be running.

To remove these entry points:

```sh
tailscale serve --http=8080 off
tailscale serve --https=443 off
```

## Configuration and restart

World atomically generates `touchpad/connection.json` when WebTransport is
enabled. It contains the advertised IP/port, certificate hash, socket index `1`,
and protocol version `1`. Only public connection information is written.

CLI options:

| Option | Default | Purpose |
| --- | --- | --- |
| `--webtransport` | off | Enable dashboard connections and generate configuration |
| `--bind IP` | `127.0.0.1` | Listening IP for both transports |
| `--advertise-ip IP` | required with WebTransport | Reachable server IP copied to browser configuration |
| `--webtransport-port PORT` | `5001` | Dashboard UDP/HTTP3 port |
| `--touchpad-config PATH` | `touchpad/connection.json` | Configuration output path, relative to the current directory |

Start world from the workspace root, or supply an explicit output path. A custom
path must be copied to `touchpad/connection.json` before Trunk builds.

Each world restart rotates the transport certificate. Trunk watches the
generated configuration and rebuilds it during development. If the served
configuration is stale, restart Trunk. The dashboard's **Reconnect** button
reloads configuration without caching and clears the old world snapshot.
Disconnected displays retain the last snapshot, marked **STALE**.

If world logs `CERTIFICATE_VERIFY_FAILED` or `certificate unknown`, check that
`connection.json` and `dist/connection.json` contain the same certificate hash.
An old copy in `dist` pins the certificate from a previous world run. Keep world
running and wait for Trunk to report a successful build before reconnecting.
To refresh only the configuration without rebuilding the WASM assets, run this
from `touchpad` while Trunk is serving:

```sh
cp connection.json dist/connection.json
```

Already-open dashboards keep the certificate pin loaded for their current
connection; copying the file does not replace that in-memory pin. Click
**Reconnect** to fetch the new configuration, or reload the page.
In PowerShell, use
`Copy-Item connection.json dist/connection.json -Force`. Repeat this after each
world restart when serving previously built assets.

Browsers may suspend background tabs, especially on mobile devices. If a tab's
connection expires while suspended, return to it and use **Reconnect**.
Release builds also reduce browser startup time and download size.

Protocol changes require rebuilding world, observer, and touchpad together.

## Controls

- Mouse drag / finger drag: pan the map.
- Mouse wheel over the map / pinch: zoom around the cursor or gesture.
- Click / tap a marker or human row: select the same human in both views.
- **Fit all**: frame every human; **+ / −**: zoom around the map center.
- Wheel / finger drag over the human list: scroll the list.

Colors are derived from server-issued human IDs, independent of query order.
The map is the existing unbounded coordinate plane, without terrain or tiles.

## Build and verification

Start world to generate configuration before building web assets:

```sh
cargo check --workspace --locked
cargo clippy --workspace --locked -- -D warnings
cargo check -p touchpad --target wasm32-unknown-unknown --locked
cargo clippy -p touchpad --target wasm32-unknown-unknown --locked -- -D warnings
cargo fmt --all -- --check
cd touchpad
trunk build --release
```

Serve `touchpad/dist` with HTTPS. Rebuild after world restarts to copy the new
certificate hash; Trunk's release build does not update configuration itself.
Certificates, generated connection configuration, and web build assets are
ignored by Git.

Smoke check with one observer and two dashboard tabs: expect three humans,
three total connections, one player, and two dashboards. Move and stop the
observer's human; both dashboards should agree on position and movement.
Close observer to verify ownership release and player-count decrement. Verify
late joins, tab closure, restart/reconnect, map/list selection, touch gestures,
window resizing, and a second LAN device.
