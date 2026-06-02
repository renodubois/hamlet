#!/usr/bin/env bash
set -euo pipefail

out=${1:-${HAMLET_LIVEKIT_CONFIG:-./livekit.local.yaml}}
server_port=${HAMLET_SERVER_PORT:-3030}
livekit_port=${HAMLET_LIVEKIT_PORT:-7880}
livekit_tcp_port=${HAMLET_LIVEKIT_TCP_PORT:-7881}
livekit_udp_start=${HAMLET_LIVEKIT_UDP_START:-50000}
livekit_udp_end=${HAMLET_LIVEKIT_UDP_END:-50100}
livekit_api_key=${LIVEKIT_API_KEY:-devkey}
livekit_api_secret=${LIVEKIT_API_SECRET:-devsecretdevsecretdevsecretdevsecret}
livekit_log_level=${LIVEKIT_LOG_LEVEL:-info}

mkdir -p "$(dirname "$out")"
cat > "$out" <<EOF
port: ${livekit_port}
bind_addresses:
  - ""
rtc:
  tcp_port: ${livekit_tcp_port}
  port_range_start: ${livekit_udp_start}
  port_range_end: ${livekit_udp_end}
  use_external_ip: false
  # In Compose, LiveKit uses host networking so it can auto-advertise a
  # non-loopback host ICE address that browsers can reach.
keys:
  ${livekit_api_key}: ${livekit_api_secret}
# Participant events drive the sidebar's "who's in this channel" UI. LiveKit
# signs these with the API secret above, and the server's /livekit/webhook
# handler verifies the signature before updating state.
webhook:
  api_key: ${livekit_api_key}
  urls:
    - http://127.0.0.1:${server_port}/livekit/webhook
logging:
  level: ${livekit_log_level}
EOF

printf 'wrote %s\n' "$out"
