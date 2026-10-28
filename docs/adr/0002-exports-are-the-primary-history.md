# Exports are the primary history, and API Connections are opt-in extras

We expected to fetch history over OAuth. The APIs cannot do it, and Spotify's terms restrict the rest:

- YouTube Data API has no watch history. The `watchHistory` playlist was deprecated in 2016 and removed. Takeout is the only complete source.
- Spotify's recently played endpoint returns at most 50 tracks. Only the data download holds lifetime history.
- Spotify Development Mode apps need the owner to hold Premium and allow 5 users per app, and Extended Quota is out of reach for a project like this. A shared client ID cannot work.
- The Spotify Developer Policy (effective 15 May 2025) says: "Do not analyze the Spotify Content or the Spotify Service for any purpose, including without limitation, creating new or derived listenership metrics" and "Do not use the Spotify Platform or any Spotify Content to train a machine learning or AI model or otherwise ingest Spotify Content into a machine learning or AI model."

So an Export is the recommended path. It needs no developer account and works without Premium, and the file is the user's own personal data, delivered under their data rights instead of through the Spotify Platform.

A Spotify Connection stays available because connecting was a stated requirement, but it is opt-in. The user registers their own client ID and reads a plain warning that quotes those two clauses, then ticks a box. Rostery refuses to connect without that consent. Each Play records its Origin, so disconnecting deletes exactly the API data, and an Export replaces overlapping API Plays.

A YouTube Connection adds subscriptions, likes and channel topics. YouTube's policies require stored API data to be refreshed or deleted within 30 calendar days, so Rostery purges that data on startup once it is older than that. Takeout data is not API data and stays.

## Considered options

- **API only.** Rejected: no YouTube history, 50 Spotify plays, and every feature would sit inside the restricted area.
- **Drop the Spotify Connection.** Compliant and simpler, but it removes something the project owner asked for. This is the option to take if Spotify objects.
- **Run a backend with one registered app.** Rejected: 5 user limit, and it breaks the promise that nothing runs anywhere but the user's machine.

## Consequences

This is not legal advice. A user who connects Spotify accepts the developer terms personally, as the holder of that client ID. The project publishes code that makes this possible, so Spotify could object to it. The maintainer has been told and owns this decision.
