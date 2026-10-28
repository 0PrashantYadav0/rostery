# 10: One-click GitHub Pages

**What to build:** Connect GitHub once, or reuse a logged-in `gh`, then publish the Share Site to GitHub Pages with one click and get a live URL. Publishing again updates the same site.

**Blocked by:** 09

**Status:** ready-for-agent

- [ ] A logged-in `gh` connects with one click, otherwise a token is pasted once and kept in the keychain
- [ ] The first publish creates the repo and enables Pages, and later ones add a commit
- [ ] Rostery refuses to publish over a repo it did not create
- [ ] The app says the repo and site are public and asks before publishing
- [ ] A bad token gets a message saying to reconnect
