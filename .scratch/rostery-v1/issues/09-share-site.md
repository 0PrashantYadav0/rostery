# 09: Generate the Share Site

**What to build:** Choose what to share and get a compiled HTML, CSS and JS folder holding your Roast, Verdict and Universes. Open it from disk or any web host. Nothing else from your history is in it, and it talks to no third party.

**Blocked by:** 05, 08

**Status:** done

- [x] The user picks a display name and whether to include the Roast, each Universe, and Item titles. The Roast is on and Item titles are off by default
- [x] The screen says, beside the choice, that the Roast quotes titles and names from all Sources, whatever else is chosen
- [x] The site works from `file://`, from a subpath and from a domain root
- [x] What was left out appears in no file, even when a stored Roast quotes it, unless that Roast is included
- [x] A hostile title cannot execute anything
- [x] The site makes no third-party requests and carries a strict CSP and `noindex`
- [x] It works on a phone, and `share.js` is under 350 KB gzipped
- [x] The app offers Preview in browser and Reveal folder
