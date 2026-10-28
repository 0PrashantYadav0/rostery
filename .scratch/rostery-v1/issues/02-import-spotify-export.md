# 02: Import a Spotify Export

**What to build:** Drop the zip or JSON from Spotify's "Download your data" onto the app and see your lifetime listening counted, with a plain summary of what was added and what was left out. Works for both the account format and the extended format.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] A zip, a JSON file or a folder all import
- [ ] Private sessions are left out and the summary says how many
- [ ] IP addresses and usernames in the file never reach the database
- [ ] Importing the same data twice, or both formats of it, does not double count
- [ ] A file Rostery does not recognise gets a message saying which Exports it accepts
- [ ] 300,000 Plays import in under 30 seconds in a release build
