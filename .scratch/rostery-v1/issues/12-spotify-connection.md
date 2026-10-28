# 12: Spotify Connection

**What to build:** Connect a Spotify account with your own client ID, after reading why an Export is better and accepting the policy risk, and sync recent Plays and top artists with genres. Disconnecting removes the token and every piece of API data. See ADR 0002.

**Blocked by:** 02, 10

**Status:** ready-for-agent

- [ ] Rostery will not connect without explicit consent that quotes Spotify's policy
- [ ] OAuth uses PKCE on the fixed loopback redirect and rejects a wrong `state`
- [ ] Sync adds recent Plays and top artists, and is incremental
- [ ] API Plays never overlap with an Export
- [ ] Disconnect removes the API data and leaves the Export data
- [ ] An expired or revoked token gets a message saying to reconnect
