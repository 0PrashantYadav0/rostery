# 04: Universe engine and first render

**What to build:** Open the Universe screen and see your Creators as 3D Planets. Size follows time spent, Links follow shared Sessions and Interests, close Planets group into Galaxies. The picture is computed in Rust and is the same every time.

**Blocked by:** 01

**Status:** done

- [x] The most played Creator has the largest Planet, and size follows the square root of time
- [x] Creators that share Sessions or Interests are linked, and one coincidental Session is not enough
- [x] Linked Planets form Galaxies
- [x] Planets never overlap and positions are never `NaN`, even with one Play or none
- [x] The Universe for 300,000 Plays builds in under 3 seconds in a release build
- [x] The Universe screen draws a real 3D scene from this data
