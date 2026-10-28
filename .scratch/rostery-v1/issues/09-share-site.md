# 09: Generate the Share Site

**What to build:** Choose what to share and get a compiled HTML, CSS and JS folder holding your Roast, Verdict and Universes. Open it from disk or any web host. Nothing else from your history is in it, and it talks to no third party.

**Blocked by:** 05, 08

**Status:** ready-for-agent

- [ ] The user picks a display name and whether to include the Roast, each Universe, and Item titles (off by default)
- [ ] The site works from `file://`, from a subpath and from a domain root
- [ ] What was left out appears in no file
- [ ] A hostile title cannot execute anything
- [ ] The site makes no third-party requests and carries a strict CSP and `noindex`
- [ ] It works on a phone, and `share.js` is under 350 KB gzipped
- [ ] The app offers Preview in browser and Reveal folder
