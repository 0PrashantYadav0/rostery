# 13: YouTube Connection

**What to build:** Connect a Google account with your own OAuth client and add subscriptions, likes and channel topics to the Universe. The screen says up front that YouTube's API has no watch history and Takeout is still needed.

**Blocked by:** 12

**Status:** done

- [x] Subscriptions and likes become Affinities, and channel topics become Interests
- [x] API data older than 30 days is removed on startup, and Takeout Plays stay
- [x] Disconnect removes the API data
- [x] Quota and expired-token errors get useful messages
- [x] Links to the YouTube Terms and Google Privacy Policy are shown

Stop point 5 follows this ticket.

The sign-in itself needs a Google Cloud app, so it is a stop point 5 check for the user.
